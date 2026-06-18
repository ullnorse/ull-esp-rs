use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer, with_timeout};
use esp_bootloader_esp_idf::ota::OtaImageState;
use heapless::String as HeaplessString;
use picoserve::ResponseSent;
use picoserve::io::Read as _;
use picoserve::request::{Headers, Request};
use picoserve::response::{Content, IntoResponse, ResponseWriter, StatusCode};
use picoserve::routing::{RequestHandlerService, get_service, post_service};
use sha2::{Digest, Sha256};
use static_cell::StaticCell;
use ull_esp_board_devkit_v1::{Board, StatusLed};
use ull_esp_platform::config::WifiConfig;
use ull_esp_platform::ota::{self, BootStatus, PartitionWriter, UpdateTarget};
use ull_esp_platform::runtime;

use crate::error::AppError;

const HTTP_LISTENER_COUNT: u8 = 2;
const HTTP_BUFFER_SIZE: usize = 1536;
const TCP_BUFFER_SIZE: usize = 2048;
const READ_BUFFER_SIZE: usize = 1024;
const HEALTH_CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);
const REBOOT_DELAY: Duration = Duration::from_secs(1);
const STATUS_LED_BLINK_PERIOD: Duration = Duration::from_millis(100);

static HTTP_SERVER_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static OTA_SERVER_STATE: StaticCell<OtaServerState> = StaticCell::new();

static HTTP_CONFIG: picoserve::Config = picoserve::Config::new(picoserve::Timeouts {
    start_read_request: Duration::from_secs(5),
    persistent_start_read_request: Duration::from_secs(3),
    read_request: Duration::from_secs(30),
    write: Duration::from_secs(10),
})
.close_connection_after_response();

pub async fn run(spawner: Spawner) -> Result<(), AppError> {
    runtime::init_default_heap();
    let config = AppConfig::from_env()?;

    let mut board = Board::init();
    board.start_runtime()?;

    let status_led = board.take_status_led()?;
    spawn_status_led_task(spawner, status_led)?;

    let wifi = board.take_wifi_station(spawner, &config.wifi)?;
    let flash = board.take_flash_storage()?;
    let state = OTA_SERVER_STATE.init(OtaServerState::new(
        config.ota_token,
        config.ota_port,
        flash,
    ));

    log_firmware_identity();

    let boot_status = {
        let mut flash = state.flash.lock().await;

        if ota::bootstrap_factoryless_otadata(&mut flash)? {
            log::info!("initialized otadata for factoryless OTA layout");
        }

        log_otadata_debug(&mut flash);

        ota::boot_status(&mut flash)?
    };
    log_boot_status(boot_status);
    state.record_boot_status(boot_status);

    log::info!("Waiting for network configuration...");
    wifi.stack().wait_config_up().await;

    spawn_http_servers(spawner, wifi.stack(), state)?;

    if boot_status.ota_state == Some(OtaImageState::PendingVerify) {
        confirm_boot_or_rollback(wifi.stack(), state).await;
    }

    Ok(())
}

fn spawn_status_led_task(spawner: Spawner, status_led: StatusLed) -> Result<(), AppError> {
    log::info!("Status LED heartbeat enabled: 1s blink period");
    let task = status_led_task(status_led).map_err(|_| AppError::TaskSpawn("status-led"))?;
    spawner.spawn(task);
    Ok(())
}

fn spawn_http_servers(
    spawner: Spawner,
    stack: Stack<'static>,
    state: &'static OtaServerState,
) -> Result<(), AppError> {
    for listener_id in 0..HTTP_LISTENER_COUNT {
        let task =
            http_server_task(listener_id, stack, state).map_err(|_| AppError::TaskSpawn("http"))?;
        spawner.spawn(task);
    }

    Ok(())
}

async fn confirm_boot_or_rollback(stack: Stack<'static>, state: &'static OtaServerState) {
    let update_path_ready = with_timeout(HEALTH_CONFIRM_TIMEOUT, async {
        stack.wait_config_up().await;
        HTTP_SERVER_READY.wait().await;
    })
    .await;

    if update_path_ready.is_err() {
        rollback_pending_boot(state, "startup health confirmation timed out").await;
    }

    let confirmation_result = {
        let mut flash = state.flash.lock().await;
        ota::mark_running_state(&mut flash, OtaImageState::Valid)
    };

    if let Err(err) = confirmation_result {
        log::error!("failed to confirm healthy image: {err}");
        rollback_pending_boot(state, "failed to confirm healthy image").await;
    }

    state.set_ota_state(Some(OtaImageState::Valid));
}

async fn rollback_pending_boot(state: &'static OtaServerState, reason: &'static str) -> ! {
    log::error!("{reason}");

    let invalidation_result = {
        let mut flash = state.flash.lock().await;
        ota::mark_running_state(&mut flash, OtaImageState::Invalid)
    };

    if let Err(err) = invalidation_result {
        log::error!("failed to mark running image invalid: {err}");
    }

    Timer::after(REBOOT_DELAY).await;
    esp_hal::system::software_reset();
}

#[embassy_executor::task]
async fn status_led_task(mut status_led: StatusLed) {
    let half_period = STATUS_LED_BLINK_PERIOD / 2;

    log::info!("Status LED task started");

    loop {
        status_led.on();
        Timer::after(half_period).await;
        status_led.off();
        Timer::after(half_period).await;
    }
}

fn log_boot_status(status: BootStatus) {
    log::info!(
        "Boot status: running={:?}, selected={:?}, ota_state={:?}",
        status.booted_partition,
        status.selected_partition,
        status.ota_state,
    );

    if status.booted_partition != status.selected_partition {
        log::warn!("booted partition differs from selected partition");
    }
}

fn log_firmware_identity() {
    let desc = &crate::ESP_APP_DESC;
    log::info!(
        "Firmware: project={}, version={}, build_id={}, built={} {}",
        desc.project_name(),
        desc.version(),
        env!("HTTP_OTA_BUILD_ID"),
        desc.date(),
        desc.time(),
    );
}

fn log_otadata_debug(flash: &mut ota::FlashStorageDevice) {
    match ota::otadata_debug_status(flash) {
        Ok(status) => {
            log::info!(
                "otadata: active_slot={:?}, slot0=(seq={}, state={:?}, state_raw=0x{:08x}, crc=0x{:08x}, expected_crc=0x{:08x}, valid={}), slot1=(seq={}, state={:?}, state_raw=0x{:08x}, crc=0x{:08x}, expected_crc=0x{:08x}, valid={})",
                status.active_slot,
                status.slot0.sequence,
                status.slot0.state,
                status.slot0.state_raw,
                status.slot0.crc,
                status.slot0.expected_crc,
                status.slot0.valid,
                status.slot1.sequence,
                status.slot1.state,
                status.slot1.state_raw,
                status.slot1.crc,
                status.slot1.expected_crc,
                status.slot1.valid,
            );
        }
        Err(err) => {
            log::warn!("failed to inspect otadata: {err}");
        }
    }
}

#[embassy_executor::task(pool_size = 2)]
async fn http_server_task(listener_id: u8, stack: Stack<'static>, state: &'static OtaServerState) {
    if let Some(config) = stack.config_v4() {
        log::info!(
            "OTA Server {listener_id} listening on: {:?}",
            config.address.address()
        );
    }

    let app = picoserve::Router::new()
        .route("/ota/status", get_service(StatusService))
        .route("/ota", post_service(UploadService))
        .with_state(state);
    let mut http_buffer = [0u8; HTTP_BUFFER_SIZE];
    let mut tcp_rx_buffer = [0u8; TCP_BUFFER_SIZE];
    let mut tcp_tx_buffer = [0u8; TCP_BUFFER_SIZE];

    HTTP_SERVER_READY.signal(());

    let _ = picoserve::Server::new(&app, &HTTP_CONFIG, &mut http_buffer)
        .listen_and_serve(
            listener_id,
            stack,
            state.port(),
            &mut tcp_rx_buffer,
            &mut tcp_tx_buffer,
        )
        .await;
}

struct AppConfig {
    wifi: WifiConfig<'static>,
    ota_token: &'static str,
    ota_port: u16,
}

impl AppConfig {
    fn from_env() -> Result<Self, AppError> {
        Ok(Self {
            wifi: WifiConfig::new(
                option_env!("WIFI_SSID").ok_or(AppError::MissingWifiSsid)?,
                option_env!("WIFI_PASSWORD").ok_or(AppError::MissingWifiPassword)?,
            ),
            ota_token: option_env!("OTA_TOKEN").ok_or(AppError::MissingOtaToken)?,
            ota_port: option_env!("OTA_PORT")
                .unwrap_or("8080")
                .parse()
                .map_err(|_| AppError::InvalidOtaPort)?,
        })
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum PartitionId {
    None = 0,
    Factory = 1,
    Ota0 = 2,
    Ota1 = 3,
}

impl PartitionId {
    fn from_partition(
        partition: Option<esp_bootloader_esp_idf::partitions::AppPartitionSubType>,
    ) -> Self {
        match partition {
            Some(esp_bootloader_esp_idf::partitions::AppPartitionSubType::Factory) => Self::Factory,
            Some(esp_bootloader_esp_idf::partitions::AppPartitionSubType::Ota0) => Self::Ota0,
            Some(esp_bootloader_esp_idf::partitions::AppPartitionSubType::Ota1) => Self::Ota1,
            _ => Self::None,
        }
    }

    fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(Self::Factory),
            2 => Some(Self::Ota0),
            3 => Some(Self::Ota1),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Factory => "factory",
            Self::Ota0 => "ota_0",
            Self::Ota1 => "ota_1",
            Self::None => "unknown",
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum CachedOtaState {
    None = 0,
    New = 1,
    PendingVerify = 2,
    Valid = 3,
    Invalid = 4,
    Aborted = 5,
    Undefined = 6,
}

impl CachedOtaState {
    fn from_state(state: Option<OtaImageState>) -> Self {
        match state {
            Some(OtaImageState::New) => Self::New,
            Some(OtaImageState::PendingVerify) => Self::PendingVerify,
            Some(OtaImageState::Valid) => Self::Valid,
            Some(OtaImageState::Invalid) => Self::Invalid,
            Some(OtaImageState::Aborted) => Self::Aborted,
            Some(OtaImageState::Undefined) => Self::Undefined,
            None => Self::None,
        }
    }

    fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(Self::New),
            2 => Some(Self::PendingVerify),
            3 => Some(Self::Valid),
            4 => Some(Self::Invalid),
            5 => Some(Self::Aborted),
            6 => Some(Self::Undefined),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::PendingVerify => "pending_verify",
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Aborted => "aborted",
            Self::Undefined => "undefined",
            Self::None => "unknown",
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum UpdatePhase {
    Idle = 0,
    Authorizing = 1,
    Receiving = 2,
    Verifying = 3,
    Activating = 4,
    Rebooting = 5,
}

impl UpdatePhase {
    fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Authorizing,
            2 => Self::Receiving,
            3 => Self::Verifying,
            4 => Self::Activating,
            5 => Self::Rebooting,
            _ => Self::Idle,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Authorizing => "authorizing",
            Self::Receiving => "receiving",
            Self::Verifying => "verifying",
            Self::Activating => "activating",
            Self::Rebooting => "rebooting",
        }
    }
}

#[derive(Copy, Clone)]
struct StatusSnapshot {
    running_partition: Option<PartitionId>,
    selected_partition: Option<PartitionId>,
    ota_state: Option<CachedOtaState>,
    update_in_progress: bool,
    phase: UpdatePhase,
    bytes_received: u32,
    content_length: u32,
}

struct OtaServerState {
    token: &'static str,
    port: u16,
    flash: Mutex<CriticalSectionRawMutex, ota::FlashStorageDevice>,
    running_partition: AtomicU8,
    selected_partition: AtomicU8,
    ota_state: AtomicU8,
    update_in_progress: AtomicBool,
    phase: AtomicU8,
    bytes_received: AtomicU32,
    content_length: AtomicU32,
}

impl OtaServerState {
    fn new(token: &'static str, port: u16, flash: ota::FlashStorageDevice) -> Self {
        Self {
            token,
            port,
            flash: Mutex::new(flash),
            running_partition: AtomicU8::new(PartitionId::None as u8),
            selected_partition: AtomicU8::new(PartitionId::None as u8),
            ota_state: AtomicU8::new(CachedOtaState::None as u8),
            update_in_progress: AtomicBool::new(false),
            phase: AtomicU8::new(UpdatePhase::Idle as u8),
            bytes_received: AtomicU32::new(0),
            content_length: AtomicU32::new(0),
        }
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn token(&self) -> &'static str {
        self.token
    }

    fn record_boot_status(&self, status: BootStatus) {
        self.running_partition.store(
            PartitionId::from_partition(status.booted_partition) as u8,
            Ordering::SeqCst,
        );
        self.selected_partition.store(
            PartitionId::from_partition(status.selected_partition) as u8,
            Ordering::SeqCst,
        );
        self.ota_state.store(
            CachedOtaState::from_state(status.ota_state) as u8,
            Ordering::SeqCst,
        );
    }

    fn set_selected_partition(
        &self,
        partition: Option<esp_bootloader_esp_idf::partitions::AppPartitionSubType>,
    ) {
        self.selected_partition.store(
            PartitionId::from_partition(partition) as u8,
            Ordering::SeqCst,
        );
    }

    fn set_ota_state(&self, state: Option<OtaImageState>) {
        self.ota_state
            .store(CachedOtaState::from_state(state) as u8, Ordering::SeqCst);
    }

    fn try_begin_update(&self, content_length: u32) -> Option<UpdateSessionGuard<'_>> {
        self.update_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        self.phase
            .store(UpdatePhase::Authorizing as u8, Ordering::SeqCst);
        self.bytes_received.store(0, Ordering::SeqCst);
        self.content_length.store(content_length, Ordering::SeqCst);

        Some(UpdateSessionGuard {
            state: self,
            armed: true,
        })
    }

    fn set_phase(&self, phase: UpdatePhase) {
        self.phase.store(phase as u8, Ordering::SeqCst);
    }

    fn set_progress(&self, bytes_received: u32) {
        self.bytes_received.store(bytes_received, Ordering::SeqCst);
    }

    fn reset_update(&self) {
        self.update_in_progress.store(false, Ordering::SeqCst);
        self.phase.store(UpdatePhase::Idle as u8, Ordering::SeqCst);
        self.bytes_received.store(0, Ordering::SeqCst);
        self.content_length.store(0, Ordering::SeqCst);
    }

    fn snapshot(&self) -> StatusSnapshot {
        StatusSnapshot {
            running_partition: PartitionId::from_raw(self.running_partition.load(Ordering::SeqCst)),
            selected_partition: PartitionId::from_raw(
                self.selected_partition.load(Ordering::SeqCst),
            ),
            ota_state: CachedOtaState::from_raw(self.ota_state.load(Ordering::SeqCst)),
            update_in_progress: self.update_in_progress.load(Ordering::SeqCst),
            phase: UpdatePhase::from_raw(self.phase.load(Ordering::SeqCst)),
            bytes_received: self.bytes_received.load(Ordering::SeqCst),
            content_length: self.content_length.load(Ordering::SeqCst),
        }
    }
}

struct UpdateSessionGuard<'a> {
    state: &'a OtaServerState,
    armed: bool,
}

impl UpdateSessionGuard<'_> {
    fn set_phase(&self, phase: UpdatePhase) {
        self.state.set_phase(phase);
    }

    fn set_progress(&self, bytes_received: u32) {
        self.state.set_progress(bytes_received);
    }
}

impl Drop for UpdateSessionGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.state.reset_update();
        }
    }
}

struct JsonBody<const N: usize>(HeaplessString<N>);

impl<const N: usize> Content for JsonBody<N> {
    fn content_type(&self) -> &'static str {
        "application/json; charset=utf-8"
    }

    fn content_length(&self) -> usize {
        self.0.len()
    }

    async fn write_content<W: picoserve::io::Write>(self, writer: W) -> Result<(), W::Error> {
        self.0.as_str().write_content(writer).await
    }
}

struct StatusService;
struct UploadService;

impl RequestHandlerService<OtaServerState> for StatusService {
    async fn call_request_handler_service<
        R: picoserve::io::Read,
        W: ResponseWriter<Error = R::Error>,
    >(
        &self,
        state: &OtaServerState,
        (): (),
        request: Request<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        if !is_authorized(request.parts.headers(), state.token()) {
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(401),
                "unauthorized",
            )
            .await;
        }

        let body = status_body(state.snapshot());
        write_json_response(request, response_writer, StatusCode::OK, body).await
    }
}

impl RequestHandlerService<OtaServerState> for UploadService {
    async fn call_request_handler_service<
        R: picoserve::io::Read,
        W: ResponseWriter<Error = R::Error>,
    >(
        &self,
        state: &OtaServerState,
        (): (),
        mut request: Request<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        if !is_authorized(request.parts.headers(), state.token()) {
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(401),
                "unauthorized",
            )
            .await;
        }

        if request.parts.headers().get("transfer-encoding").is_some() {
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(400),
                "transfer-encoding is not supported",
            )
            .await;
        }

        if request.parts.headers().get("content-length").is_none() {
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(411),
                "content-length is required",
            )
            .await;
        }

        let content_length = request.body_connection.content_length();
        if content_length == 0 {
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(400),
                "empty upload",
            )
            .await;
        }

        let expected_digest = match request.parts.headers().get("x-ota-sha256") {
            Some(value) => match value.as_str().ok().and_then(parse_sha256_hex) {
                Some(digest) => digest,
                None => {
                    return write_error_response(
                        request,
                        response_writer,
                        StatusCode::new(400),
                        "x-ota-sha256 must be 64 lowercase hex characters",
                    )
                    .await;
                }
            },
            None => {
                return write_error_response(
                    request,
                    response_writer,
                    StatusCode::new(400),
                    "missing x-ota-sha256",
                )
                .await;
            }
        };

        let content_length = content_length as u32;
        let Some(session) = state.try_begin_update(content_length) else {
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(409),
                "an update is already in progress",
            )
            .await;
        };

        let mut flash = state.flash.lock().await;
        let target = match ota::next_update_target(&mut flash) {
            Ok(target) => target,
            Err(err) => {
                log::error!("failed to select OTA target: {err}");
                return write_error_response(
                    request,
                    response_writer,
                    StatusCode::new(500),
                    "failed to select OTA target",
                )
                .await;
            }
        };

        if content_length > target.size {
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(413),
                "application image does not fit target partition",
            )
            .await;
        }

        let mut writer = match PartitionWriter::begin(&mut flash, target, content_length) {
            Ok(writer) => writer,
            Err(err) => {
                log::error!("failed to prepare OTA partition: {err}");
                return write_error_response(
                    request,
                    response_writer,
                    StatusCode::new(500),
                    "failed to prepare OTA partition",
                )
                .await;
            }
        };

        let upload_result = {
            let mut reader = request.body_connection.body().reader();
            let mut read_buffer = [0u8; READ_BUFFER_SIZE];
            let mut prefix = [0u8; ota::APP_IMAGE_PREFIX_LEN];
            let mut prefix_len = 0usize;
            let mut prefix_checked = false;
            let mut hasher = Sha256::new();

            session.set_phase(UpdatePhase::Receiving);

            loop {
                let read = match reader.read(&mut read_buffer).await {
                    Ok(read) => read,
                    Err(err) => {
                        log::error!("failed to read upload body: {err:?}");
                        break Err((StatusCode::new(500), "failed to read upload body"));
                    }
                };

                if read == 0 {
                    break Ok((prefix_checked, hasher));
                }

                let chunk = &read_buffer[..read];
                let prefix_remaining = prefix.len().saturating_sub(prefix_len);
                if prefix_remaining > 0 {
                    let take = prefix_remaining.min(chunk.len());
                    prefix[prefix_len..prefix_len + take].copy_from_slice(&chunk[..take]);
                    prefix_len += take;

                    if !prefix_checked && prefix_len == prefix.len() {
                        if let Err(err) = ota::validate_app_image_prefix(&prefix) {
                            log::warn!("rejecting invalid upload prefix: {err}");
                            break Err((StatusCode::new(400), "invalid firmware image header"));
                        }
                        prefix_checked = true;
                    }
                }

                hasher.update(chunk);
                if let Err(err) = writer.write_chunk(&mut flash, chunk) {
                    log::error!("failed to write OTA chunk: {err}");
                    break Err((StatusCode::new(500), "failed to write OTA image"));
                }

                session.set_progress(writer.bytes_received());
            }
        };

        let (prefix_checked, hasher) = match upload_result {
            Ok(result) => result,
            Err((status, message)) => {
                let _ = ota::erase_target_partition(&mut flash, &target);
                return write_error_response(request, response_writer, status, message).await;
            }
        };

        if writer.bytes_received() != content_length {
            let _ = ota::erase_target_partition(&mut flash, &target);
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(400),
                "truncated upload",
            )
            .await;
        }

        if !prefix_checked {
            let _ = ota::erase_target_partition(&mut flash, &target);
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(400),
                "upload ended before the image header could be validated",
            )
            .await;
        }

        if let Err(err) = writer.finish(&mut flash) {
            log::error!("failed to flush OTA image: {err}");
            let _ = ota::erase_target_partition(&mut flash, &target);
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(500),
                "failed to finish OTA write",
            )
            .await;
        }

        session.set_phase(UpdatePhase::Verifying);

        let actual_digest = hasher.finalize();
        if actual_digest.as_slice() != expected_digest.as_slice() {
            log::warn!("rejecting upload with mismatched SHA-256");
            let _ = ota::erase_target_partition(&mut flash, &target);
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(400),
                "sha256 mismatch",
            )
            .await;
        }

        session.set_phase(UpdatePhase::Activating);
        if let Err(err) = ota::activate_partition(&mut flash, target.partition, OtaImageState::New)
        {
            log::error!("failed to activate OTA partition: {err}");
            let _ = ota::erase_target_partition(&mut flash, &target);
            return write_error_response(
                request,
                response_writer,
                StatusCode::new(500),
                "failed to activate OTA image",
            )
            .await;
        }

        log_otadata_debug(&mut flash);

        state.set_selected_partition(Some(target.partition));
        state.set_ota_state(Some(OtaImageState::New));
        session.set_phase(UpdatePhase::Rebooting);

        let response = upload_success_body(target);
        let connection = request.body_connection.finalize().await?;
        let _response_sent = (StatusCode::OK, response)
            .write_to(connection, response_writer)
            .await?;

        Timer::after(REBOOT_DELAY).await;
        esp_hal::system::software_reset();
    }
}

fn is_authorized(headers: Headers<'_>, expected_token: &str) -> bool {
    headers
        .get("x-ota-token")
        .and_then(|value| value.as_str().ok())
        == Some(expected_token)
}

async fn write_error_response<R: picoserve::io::Read, W: ResponseWriter<Error = R::Error>>(
    request: Request<'_, R>,
    response_writer: W,
    status: StatusCode,
    message: &'static str,
) -> Result<ResponseSent, W::Error> {
    write_json_response(request, response_writer, status, error_body(message)).await
}

async fn write_json_response<
    R: picoserve::io::Read,
    W: ResponseWriter<Error = R::Error>,
    const N: usize,
>(
    request: Request<'_, R>,
    response_writer: W,
    status: StatusCode,
    body: JsonBody<N>,
) -> Result<ResponseSent, W::Error> {
    let connection = request.body_connection.finalize().await?;
    (status, body).write_to(connection, response_writer).await
}

fn error_body(message: &'static str) -> JsonBody<160> {
    let mut body = HeaplessString::new();
    let _ = write!(body, "{{\"error\":\"{}\"}}", message);
    JsonBody(body)
}

fn upload_success_body(target: UpdateTarget) -> JsonBody<192> {
    let mut body = HeaplessString::new();
    let target_partition = PartitionId::from_partition(Some(target.partition)).as_str();
    let _ = write!(
        body,
        "{{\"status\":\"ok\",\"target_partition\":\"{}\",\"rebooting\":true}}",
        target_partition,
    );
    JsonBody(body)
}

fn status_body(snapshot: StatusSnapshot) -> JsonBody<512> {
    let mut body = HeaplessString::new();
    let desc = &crate::ESP_APP_DESC;
    let _ = write!(
        body,
        "{{\"firmware_project\":\"{}\",\"firmware_version\":\"{}\",\"firmware_build_id\":\"{}\",\"firmware_build_date\":\"{}\",\"firmware_build_time\":\"{}\",\"running_partition\":",
        desc.project_name(),
        desc.version(),
        env!("HTTP_OTA_BUILD_ID"),
        desc.date(),
        desc.time(),
    );
    push_json_string_or_null(
        &mut body,
        snapshot.running_partition.map(PartitionId::as_str),
    );
    let _ = write!(body, ",\"selected_partition\":");
    push_json_string_or_null(
        &mut body,
        snapshot.selected_partition.map(PartitionId::as_str),
    );
    let _ = write!(body, ",\"ota_state\":");
    push_json_string_or_null(&mut body, snapshot.ota_state.map(CachedOtaState::as_str));
    let _ = write!(
        body,
        ",\"update_in_progress\":{},\"update_mode\":{},\"bytes_received\":{},\"content_length\":{},\"phase\":\"{}\"}}",
        snapshot.update_in_progress,
        snapshot.phase != UpdatePhase::Idle,
        snapshot.bytes_received,
        snapshot.content_length,
        snapshot.phase.as_str(),
    );
    JsonBody(body)
}

fn push_json_string_or_null<const N: usize>(body: &mut HeaplessString<N>, value: Option<&str>) {
    match value {
        Some(value) => {
            let _ = write!(body, "\"{}\"", value);
        }
        None => {
            let _ = write!(body, "null");
        }
    }
}

fn parse_sha256_hex(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }

    let mut digest = [0u8; 32];
    let bytes = value.as_bytes();

    let mut index = 0;
    while index < digest.len() {
        let high = decode_lower_hex(bytes[index * 2])?;
        let low = decode_lower_hex(bytes[index * 2 + 1])?;
        digest[index] = (high << 4) | low;
        index += 1;
    }

    Some(digest)
}

fn decode_lower_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
