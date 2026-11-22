use core::fmt::Write as _;

use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_net::tcp::TcpSocket;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer, with_timeout};
use heapless::String as HeaplessString;
use static_cell::StaticCell;
use ull_esp_board_devkit_v1::{Board, StatusLed};
use ull_esp_platform::flash::FlashStorageDevice;
use ull_esp_platform::ota::{self, BootStatus, ExpectedImage, InstalledImage, UpdateInstaller};
use ull_esp_platform::{hex, http, runtime};

use crate::config::{AppConfig, FleetConfig, OTA_PUBLIC_KEY};
use crate::error::AppError;

const TCP_BUFFER_SIZE: usize = 2048;
const HTTP_HEAD_BUFFER_SIZE: usize = 1024;
const REQUEST_BUFFER_SIZE: usize = 256;
const READ_BUFFER_SIZE: usize = 1024;
const HEALTH_CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_secs(10);
const REBOOT_DELAY: Duration = Duration::from_secs(1);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(30);
const STATUS_LED_BLINK_PERIOD: Duration = Duration::from_millis(5000);
const UPDATE_PATH: &str = "/api/update";
const SHA256_HEADER: &str = "x-application-image-sha256";
const SIGNATURE_HEADER: &str = "x-application-image-ed25519";

static OTA_CLIENT_STATE: StaticCell<OtaClientState> = StaticCell::new();

pub async fn run(spawner: Spawner) -> Result<(), AppError> {
    runtime::init_default_heap();
    let config = AppConfig::from_env()?;

    let mut board = Board::init();
    board.start_runtime()?;

    let status_led = board.take_status_led()?;
    spawn_status_led_task(spawner, status_led)?;

    let wifi = board.take_wifi_station(spawner, &config.wifi)?;
    let flash = board.take_flash_storage()?;
    let state = OTA_CLIENT_STATE.init(OtaClientState::new(flash));

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

    if boot_status.requires_health_confirmation() {
        confirm_boot_or_rollback(wifi.stack(), state).await;
    }

    spawn_update_poll_task(spawner, wifi.stack(), state, config.fleet)?;

    Ok(())
}

fn spawn_status_led_task(spawner: Spawner, status_led: StatusLed) -> Result<(), AppError> {
    log::info!("Status LED heartbeat enabled: 5s blink period");
    let task = status_led_task(status_led).map_err(|_| AppError::TaskSpawn("status-led"))?;
    spawner.spawn(task);
    Ok(())
}

fn spawn_update_poll_task(
    spawner: Spawner,
    stack: Stack<'static>,
    state: &'static OtaClientState,
    config: FleetConfig,
) -> Result<(), AppError> {
    let task =
        update_poll_task(stack, state, config).map_err(|_| AppError::TaskSpawn("update-poll"))?;
    spawner.spawn(task);
    Ok(())
}

async fn confirm_boot_or_rollback(stack: Stack<'static>, state: &'static OtaClientState) {
    if with_timeout(HEALTH_CONFIRM_TIMEOUT, stack.wait_config_up())
        .await
        .is_err()
    {
        rollback_pending_boot(state, "startup health confirmation timed out").await;
    }

    let confirmation_result = {
        let mut flash = state.flash.lock().await;
        ota::confirm_running_image(&mut flash)
    };

    if let Err(err) = confirmation_result {
        log::error!("failed to confirm healthy image: {err}");
        rollback_pending_boot(state, "failed to confirm healthy image").await;
    }
}

async fn rollback_pending_boot(state: &'static OtaClientState, reason: &'static str) -> ! {
    log::error!("{reason}");

    let invalidation_result = {
        let mut flash = state.flash.lock().await;
        ota::reject_running_image(&mut flash)
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

#[embassy_executor::task]
async fn update_poll_task(
    stack: Stack<'static>,
    state: &'static OtaClientState,
    config: FleetConfig,
) {
    log::info!(
        "Polling {} via http://{} every 10s",
        UPDATE_PATH,
        config.authority,
    );

    loop {
        if !stack.is_config_up() {
            log::info!("Waiting for network configuration...");
            stack.wait_config_up().await;
        }

        match poll_for_update(stack, state, config).await {
            Ok(()) => Timer::after(POLL_INTERVAL).await,
            Err(err) => {
                log::warn!("update poll failed: {err}");
                Timer::after(POLL_INTERVAL).await;
            }
        }
    }
}

async fn poll_for_update(
    stack: Stack<'static>,
    state: &'static OtaClientState,
    config: FleetConfig,
) -> Result<(), AppError> {
    let mut request = HeaplessString::<REQUEST_BUFFER_SIZE>::new();
    write!(
        request,
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        UPDATE_PATH, config.authority,
    )
    .map_err(|_| AppError::RequestBufferTooSmall)?;

    let mut rx_buffer = [0u8; TCP_BUFFER_SIZE];
    let mut tx_buffer = [0u8; TCP_BUFFER_SIZE];
    let mut head_buffer = [0u8; HTTP_HEAD_BUFFER_SIZE];
    let mut read_buffer = [0u8; READ_BUFFER_SIZE];
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(SOCKET_TIMEOUT));

    let result = async {
        http::send_request(
            &mut socket,
            (config.server_ip, config.port),
            request.as_bytes(),
        )
        .await?;
        let (head, initial_body) = http::read_response_head(&mut socket, &mut head_buffer).await?;

        if head.status == 204 {
            log::info!("no update available");
            return Ok(None);
        }
        if !head.is_success() {
            return Err(AppError::HttpStatus(head.status));
        }

        let expected = expected_image(&head)?;
        let mut flash = state.flash.lock().await;
        let installed = install_pending_update(
            &mut socket,
            &mut flash,
            initial_body,
            expected,
            &mut read_buffer,
        )
        .await?;
        log_otadata_debug(&mut flash);
        Ok(Some(installed))
    }
    .await;
    http::close(&mut socket).await;

    let Some(installed) = result? else {
        return Ok(());
    };

    log::info!(
        "application image activated in {:?}; rebooting",
        installed.partition,
    );

    Timer::after(REBOOT_DELAY).await;
    esp_hal::system::software_reset();
}

fn expected_image(head: &http::ResponseHead<'_>) -> Result<ExpectedImage, AppError> {
    let size = head
        .content_length()
        .map_err(http::HttpClientError::from)?
        .ok_or(AppError::MissingContentLength)?;
    let size = u32::try_from(size).map_err(|_| AppError::InvalidContentLength)?;
    let sha256 = head
        .header(SHA256_HEADER)
        .and_then(hex::decode::<32>)
        .ok_or(AppError::InvalidApplicationImageSha256)?;

    let expected = ExpectedImage::new(size, sha256);
    // A missing signature is left for the installer to reject, with its own error.
    match head.header(SIGNATURE_HEADER) {
        None => Ok(expected),
        Some(value) => hex::decode::<{ ota::SIGNATURE_LEN }>(value)
            .map(|signature| expected.with_signature(signature))
            .ok_or(AppError::InvalidApplicationImageSignature),
    }
}

async fn install_pending_update(
    socket: &mut TcpSocket<'_>,
    flash: &mut FlashStorageDevice,
    initial_body: &[u8],
    expected: ExpectedImage,
    read_buffer: &mut [u8],
) -> Result<InstalledImage, AppError> {
    let mut installer = UpdateInstaller::begin(flash, expected, Some(&OTA_PUBLIC_KEY))?;

    log::info!(
        "downloading pending OTA image ({} bytes) into {:?}",
        expected.size,
        installer.target().partition,
    );

    if !initial_body.is_empty() {
        installer.write_chunk(initial_body)?;
    }

    download_into_installer(socket, &mut installer, read_buffer).await?;

    installer.finish().map_err(Into::into)
}

async fn download_into_installer(
    socket: &mut TcpSocket<'_>,
    installer: &mut UpdateInstaller<'_>,
    read_buffer: &mut [u8],
) -> Result<(), AppError> {
    loop {
        let read = socket
            .read(read_buffer)
            .await
            .map_err(|_| AppError::TcpReadFailed)?;

        if read == 0 {
            break;
        }

        installer.write_chunk(&read_buffer[..read])?;
    }

    Ok(())
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

fn log_otadata_debug(flash: &mut FlashStorageDevice) {
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

struct OtaClientState {
    flash: Mutex<CriticalSectionRawMutex, FlashStorageDevice>,
}

impl OtaClientState {
    fn new(flash: FlashStorageDevice) -> Self {
        Self {
            flash: Mutex::new(flash),
        }
    }
}
