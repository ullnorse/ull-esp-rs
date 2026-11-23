//! Client for the ull-fleet server: periodic check-ins with metrics, signed
//! OTA updates on request, and health confirmation of a freshly updated image.
//!
//! ```ignore
//! static METRICS: FleetMetrics = FleetMetrics::new();
//! fleet::start(spawner, stack, flash, settings.server_url.as_str(), &ESP_APP_DESC, &METRICS)?;
//! // Elsewhere:
//! METRICS.set("temperature_c", 21.5);
//! ```
//!
//! The update key comes from `OTA_PUBLIC_KEY` in `~/.config/ull-esp/device.env`
//! at build time. For another key or a custom task, build an [`Agent`] and call
//! [`Agent::run`] yourself.

use core::cell::RefCell;
use core::fmt::Write as _;

use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Ipv4Address, Stack};
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use esp_bootloader_esp_idf::EspAppDesc;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::partitions::AppPartitionSubType;
use heapless::{String, Vec};
use thiserror::Error;
use ull_esp_core::fleet::{Checkin, CheckinReply, parse_reply};

pub use ull_esp_core::fleet::{DeviceId, MAX_METRICS};

use crate::error::EspError;
use crate::flash::FlashStorageDevice;
use crate::hex;
use crate::http::{self, BaseUrl, HttpClientError, ResponseHead};
use crate::ota::{
    self, ExpectedImage, InstalledImage, OtaError, PUBLIC_KEY_LEN, SIGNATURE_LEN, UpdateInstaller,
};
use crate::restart::{self, NoteKind, Restart};
use crate::watchdog::Pulse;

/// The agent's progress, for [`crate::watchdog::start`]: it beats before every
/// check-in, while it waits for the network, and per downloaded chunk.
pub static PULSE: Pulse = Pulse::new("fleet agent");

const SHA256_HEADER: &str = "x-application-image-sha256";
const SIGNATURE_HEADER: &str = "x-application-image-ed25519";
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(30);
const MIN_POLL_INTERVAL_S: u32 = 5;
const MAX_POLL_INTERVAL_S: u32 = 3600;
/// A freshly updated image must reach the server within this time, or it is
/// marked invalid and the bootloader returns to the previous image.
const HEALTH_CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(20);
const REBOOT_DELAY: Duration = Duration::from_secs(1);
/// Longest a check-in may take: connect, send, and read, each bounded by
/// [`SOCKET_TIMEOUT`], with room to spare.
const CHECKIN_BUDGET: Duration = Duration::from_secs(120);
/// How often the agent beats while it waits for the network.
const NETWORK_WAIT_STEP: Duration = Duration::from_secs(30);

/// The update key built in from `OTA_PUBLIC_KEY` (see `build.rs`). A
/// malformed key fails the build.
pub const BUILT_IN_UPDATE_KEY: Option<[u8; PUBLIC_KEY_LEN]> = match option_env!("OTA_PUBLIC_KEY") {
    Some(key) => Some(hex::decode_const(key)),
    None => None,
};

/// Metrics sized for one check-in.
pub type FleetMetrics = Metrics<MAX_METRICS>;

/// Starts the fleet agent in its own task: check-ins with `metrics`, signed
/// updates, and health confirmation. Without a server URL or a built-in
/// update key it logs a warning and starts nothing. Watch [`PULSE`] with the
/// watchdog.
pub fn start(
    spawner: Spawner,
    stack: Stack<'static>,
    flash: FlashStorageDevice,
    server_url: &'static str,
    firmware: &'static EspAppDesc,
    metrics: &'static FleetMetrics,
) -> Result<(), EspError> {
    let Some(config) = FleetConfig::parse(server_url) else {
        log::warn!("no fleet server set; running without updates");
        return Ok(());
    };
    let Some(key) = BUILT_IN_UPDATE_KEY else {
        log::warn!("no update key built in (OTA_PUBLIC_KEY); running without updates");
        return Ok(());
    };
    let agent = Agent::new(config, Firmware::from_app_desc(firmware), key, flash);
    spawner.spawn(agent_task(agent, stack, metrics).map_err(|_| EspError::TaskSpawn("fleet"))?);
    Ok(())
}

#[embassy_executor::task]
async fn agent_task(agent: Agent, stack: Stack<'static>, metrics: &'static FleetMetrics) {
    agent.run(stack, metrics).await
}

/// Where the fleet server is.
#[derive(Copy, Clone, Debug)]
pub struct FleetConfig {
    /// `host:port` for the `Host` header.
    pub authority: &'static str,
    pub server_ip: Ipv4Address,
    pub port: u16,
}

impl FleetConfig {
    /// Parses `http://192.168.1.10:3000` (IPv4 only; there is no DNS).
    pub fn parse(base_url: &'static str) -> Option<Self> {
        let url = BaseUrl::parse(base_url)?;
        Some(Self {
            authority: url.authority,
            server_ip: url.ip,
            port: url.port,
        })
    }
}

/// The running firmware's identity, from its app descriptor.
#[derive(Copy, Clone, Debug)]
pub struct Firmware {
    pub project: &'static str,
    pub version: &'static str,
    pub secure_version: u32,
}

impl Firmware {
    pub fn from_app_desc(desc: &'static EspAppDesc) -> Self {
        Self {
            project: desc.project_name(),
            version: desc.version(),
            secure_version: desc.secure_version(),
        }
    }
}

/// Latest value of each metric, shared between the tasks that measure and the
/// agent that reports. Names carry their unit: `temperature_c`, `humidity_pct`.
pub struct Metrics<const N: usize> {
    values: BlockingMutex<CriticalSectionRawMutex, RefCell<Vec<(&'static str, f32), N>>>,
}

impl<const N: usize> Metrics<N> {
    pub const fn new() -> Self {
        Self {
            values: BlockingMutex::new(RefCell::new(Vec::new())),
        }
    }

    /// Sets a metric. When all `N` slots are taken by other names, the value
    /// is dropped.
    pub fn set(&self, name: &'static str, value: f32) {
        self.values.lock(|values| {
            let mut values = values.borrow_mut();
            if let Some(slot) = values.iter_mut().find(|(existing, _)| *existing == name) {
                slot.1 = value;
            } else {
                let _ = values.push((name, value));
            }
        });
    }

    pub fn remove(&self, name: &'static str) {
        self.values.lock(|values| {
            values
                .borrow_mut()
                .retain(|(existing, _)| *existing != name)
        });
    }

    pub fn snapshot(&self) -> Vec<(&'static str, f32), N> {
        self.values.lock(|values| values.borrow().clone())
    }
}

impl<const N: usize> Default for Metrics<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error)]
pub enum FleetError {
    #[error(transparent)]
    Http(#[from] HttpClientError),
    #[error("server answered HTTP {0}")]
    Status(u16),
    #[error("check-in request does not fit its buffer")]
    RequestTooLarge,
    #[error("malformed check-in reply")]
    BadReply,
    #[error("firmware response lacks a valid Content-Length")]
    BadContentLength,
    #[error("firmware response lacks a valid SHA-256 header")]
    BadDigestHeader,
    #[error("firmware response has a malformed signature header")]
    BadSignatureHeader,
    #[error("download failed: {0}")]
    Download(HttpClientError),
    #[error(transparent)]
    Ota(#[from] OtaError),
}

pub struct Agent {
    config: FleetConfig,
    firmware: Firmware,
    public_key: [u8; PUBLIC_KEY_LEN],
    device_id: DeviceId,
    flash: FlashStorageDevice,
    /// Reported once on the next check-in after a failed update.
    last_error: String<96>,
    /// Why this boot happened, reported on every check-in.
    restart: Restart,
}

impl Agent {
    pub fn new(
        config: FleetConfig,
        firmware: Firmware,
        public_key: [u8; PUBLIC_KEY_LEN],
        flash: FlashStorageDevice,
    ) -> Self {
        let mac = esp_hal::efuse::base_mac_address();
        let mut bytes = [0u8; 6];
        bytes.copy_from_slice(mac.as_bytes());
        Self {
            config,
            firmware,
            public_key,
            device_id: DeviceId::from_mac(bytes),
            flash,
            last_error: String::new(),
            restart: restart::last(),
        }
    }

    pub fn device_id(&self) -> DeviceId {
        self.device_id
    }

    /// Checks in forever. Installs and reboots into updates the server
    /// offers. Confirms a freshly updated image after its first successful
    /// check-in, or rolls it back when it cannot reach the server in time.
    pub async fn run<const N: usize>(mut self, stack: Stack<'static>, metrics: &Metrics<N>) -> ! {
        match ota::bootstrap_factoryless_otadata(&mut self.flash) {
            Ok(true) => log::info!("initialized otadata for the factoryless OTA layout"),
            Ok(false) => {}
            Err(err) => log::warn!("otadata bootstrap failed: {err}"),
        }
        let boot = match ota::boot_status(&mut self.flash) {
            Ok(status) => Some(status),
            Err(err) => {
                log::warn!("cannot read boot status: {err}");
                None
            }
        };
        let partition = boot
            .and_then(|status| status.booted_partition)
            .map_or("", partition_name);
        let mut ota_state = boot.and_then(|status| status.ota_state);
        let mut must_confirm = boot.is_some_and(|status| status.requires_health_confirmation());
        let started = Instant::now();
        let mut poll_interval = DEFAULT_POLL_INTERVAL;

        log::info!(
            "fleet: device {} running {} {} from {partition}; server http://{}",
            self.device_id,
            self.firmware.project,
            self.firmware.version,
            self.config.authority,
        );
        match self.restart.detail() {
            Some(detail) => log::info!("fleet: last restart: {} ({detail})", self.restart.cause),
            None => log::info!("fleet: last restart: {}", self.restart.cause),
        }

        loop {
            while !stack.is_config_up() {
                PULSE.beat_within(NETWORK_WAIT_STEP + CHECKIN_BUDGET);
                if must_confirm && started.elapsed() > HEALTH_CONFIRM_TIMEOUT {
                    self.roll_back("no network after update").await;
                }
                let _ = with_timeout(NETWORK_WAIT_STEP, stack.wait_config_up()).await;
            }

            PULSE.beat_within(CHECKIN_BUDGET);
            let snapshot = metrics.snapshot();
            match self.checkin(stack, partition, ota_state, &snapshot).await {
                Ok(reply) => {
                    self.last_error.clear();
                    if must_confirm {
                        match ota::confirm_running_image(&mut self.flash) {
                            Ok(()) => {
                                log::info!("fleet: update confirmed healthy");
                                ota_state = Some(OtaImageState::Valid);
                                must_confirm = false;
                            }
                            Err(err) => {
                                log::error!("failed to confirm the running image: {err}");
                                self.roll_back("confirmation failed").await;
                            }
                        }
                    }
                    if let Some(seconds) = reply.poll_interval_s {
                        poll_interval = Duration::from_secs(
                            seconds
                                .clamp(MIN_POLL_INTERVAL_S, MAX_POLL_INTERVAL_S)
                                .into(),
                        );
                    }
                    if reply.update {
                        self.update(stack).await;
                    }
                }
                Err(err) => {
                    log::warn!("fleet check-in failed: {err}");
                    if must_confirm && started.elapsed() > HEALTH_CONFIRM_TIMEOUT {
                        self.roll_back("could not reach the fleet server after update")
                            .await;
                    }
                }
            }

            PULSE.beat_within(poll_interval + CHECKIN_BUDGET);
            Timer::after(poll_interval).await;
        }
    }

    async fn checkin(
        &mut self,
        stack: Stack<'static>,
        partition: &str,
        ota_state: Option<OtaImageState>,
        metrics: &[(&'static str, f32)],
    ) -> Result<CheckinReply, FleetError> {
        let checkin = Checkin {
            project: self.firmware.project,
            version: self.firmware.version,
            secure_version: self.firmware.secure_version,
            partition,
            ota_state: ota_state.map_or("", state_name),
            uptime_s: Instant::now().as_secs(),
            rssi_dbm: crate::wifi::rssi_dbm(),
            ip: stack
                .config_v4()
                .map(|config| config.address.address().octets()),
            last_ota_error: (!self.last_error.is_empty()).then_some(self.last_error.as_str()),
            reset_reason: self.restart.cause.as_str(),
            reset_detail: self.restart.detail(),
            metrics,
        };
        let mut body: String<1024> = String::new();
        checkin
            .write_json(&mut body)
            .map_err(|_| FleetError::RequestTooLarge)?;
        let mut request: String<1280> = String::new();
        write!(
            request,
            "POST /api/v1/devices/{}/checkin HTTP/1.1\r\nHost: {}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.device_id,
            self.config.authority,
            body.len(),
            body,
        )
        .map_err(|_| FleetError::RequestTooLarge)?;

        let mut rx_buffer = [0u8; 1024];
        let mut tx_buffer = [0u8; 1536];
        let mut head_buffer = [0u8; 512];
        let mut body_buffer = [0u8; 256];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(SOCKET_TIMEOUT));

        let result = async {
            http::send_request(
                &mut socket,
                (self.config.server_ip, self.config.port),
                request.as_bytes(),
            )
            .await?;
            let (head, prefix) = http::read_response_head(&mut socket, &mut head_buffer).await?;
            if !head.is_success() {
                return Err(FleetError::Status(head.status));
            }
            let body = http::read_small_body(&mut socket, &head, prefix, &mut body_buffer).await?;
            parse_reply(body).ok_or(FleetError::BadReply)
        }
        .await;
        http::close(&mut socket).await;
        result
    }

    /// Downloads and installs the offered release, then reboots into it.
    /// Returns only when nothing was installed.
    async fn update(&mut self, stack: Stack<'static>) {
        match self.download(stack).await {
            Ok(Some(installed)) => {
                log::info!(
                    "fleet: update installed in {:?}; rebooting",
                    installed.partition
                );
                Timer::after(REBOOT_DELAY).await;
                restart::reboot(NoteKind::Update, "");
            }
            Ok(None) => log::info!("fleet: no firmware to download"),
            Err(err) => {
                log::warn!("fleet update failed: {err}");
                self.last_error.clear();
                let _ = write!(self.last_error, "{err}");
            }
        }
    }

    async fn download(
        &mut self,
        stack: Stack<'static>,
    ) -> Result<Option<InstalledImage>, FleetError> {
        let mut request: String<256> = String::new();
        write!(
            request,
            "GET /api/v1/devices/{}/firmware HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            self.device_id, self.config.authority,
        )
        .map_err(|_| FleetError::RequestTooLarge)?;

        let mut rx_buffer = [0u8; 4096];
        let mut tx_buffer = [0u8; 512];
        let mut head_buffer = [0u8; 1024];
        let mut read_buffer = [0u8; 2048];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(SOCKET_TIMEOUT));

        let public_key = self.public_key;
        let secure_version = self.firmware.secure_version;
        let flash = &mut self.flash;
        let result = async {
            http::send_request(
                &mut socket,
                (self.config.server_ip, self.config.port),
                request.as_bytes(),
            )
            .await?;
            let (head, prefix) = http::read_response_head(&mut socket, &mut head_buffer).await?;
            if head.status == 204 {
                return Ok(None);
            }
            if !head.is_success() {
                return Err(FleetError::Status(head.status));
            }
            let expected = expected_image(&head)?;
            let mut installer = UpdateInstaller::begin(flash, expected, Some(&public_key))?
                .require_secure_version(secure_version);
            log::info!(
                "fleet: downloading {} bytes into {:?}",
                expected.size,
                installer.target().partition
            );
            if !prefix.is_empty() {
                installer.write_chunk(prefix)?;
            }
            while installer.bytes_received() < expected.size {
                let count = socket
                    .read(&mut read_buffer)
                    .await
                    .map_err(|_| FleetError::Download(HttpClientError::Read))?;
                if count == 0 {
                    break;
                }
                PULSE.beat_within(CHECKIN_BUDGET);
                installer.write_chunk(&read_buffer[..count])?;
            }
            Ok(Some(installer.finish()?))
        }
        .await;
        http::close(&mut socket).await;
        result
    }

    async fn roll_back(&mut self, reason: &str) -> ! {
        log::error!("fleet: {reason}; rolling back to the previous image");
        if let Err(err) = ota::reject_running_image(&mut self.flash) {
            log::error!("failed to mark the running image invalid: {err}");
        }
        Timer::after(REBOOT_DELAY).await;
        restart::reboot(NoteKind::Rollback, reason);
    }
}

fn expected_image(head: &ResponseHead<'_>) -> Result<ExpectedImage, FleetError> {
    let size = head
        .content_length()
        .ok()
        .flatten()
        .and_then(|size| u32::try_from(size).ok())
        .ok_or(FleetError::BadContentLength)?;
    let sha256 = head
        .header(SHA256_HEADER)
        .and_then(hex::decode::<32>)
        .ok_or(FleetError::BadDigestHeader)?;
    let expected = ExpectedImage::new(size, sha256);
    // A missing signature is rejected by the installer with its own error.
    match head.header(SIGNATURE_HEADER) {
        None => Ok(expected),
        Some(value) => hex::decode::<SIGNATURE_LEN>(value)
            .map(|signature| expected.with_signature(signature))
            .ok_or(FleetError::BadSignatureHeader),
    }
}

fn partition_name(partition: AppPartitionSubType) -> &'static str {
    const OTA: [&str; 16] = [
        "ota_0", "ota_1", "ota_2", "ota_3", "ota_4", "ota_5", "ota_6", "ota_7", "ota_8", "ota_9",
        "ota_10", "ota_11", "ota_12", "ota_13", "ota_14", "ota_15",
    ];
    match partition {
        AppPartitionSubType::Factory => "factory",
        AppPartitionSubType::Test => "test",
        ota => OTA
            .get(usize::from(
                (ota as u8).wrapping_sub(AppPartitionSubType::Ota0 as u8),
            ))
            .copied()
            .unwrap_or("ota"),
    }
}

fn state_name(state: OtaImageState) -> &'static str {
    match state {
        OtaImageState::New => "new",
        OtaImageState::PendingVerify => "pending_verify",
        OtaImageState::Valid => "valid",
        OtaImageState::Invalid => "invalid",
        OtaImageState::Aborted => "aborted",
        OtaImageState::Undefined => "undefined",
    }
}
