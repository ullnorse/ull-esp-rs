use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use ull_esp_board_devkit_v1::Board;
use ull_esp_platform::fleet::{Agent, Firmware, FleetConfig, Metrics};
use ull_esp_platform::ota::PUBLIC_KEY_LEN;
use ull_esp_platform::settings::{self, Settings};
use ull_esp_platform::setup;
use ull_esp_platform::watchdog::Pulse;
use ull_esp_platform::{config::WifiConfig, hex, runtime};

use crate::error::AppError;
use crate::tasks::{display, fleet, sensor};

/// Ed25519 key that every update must be signed with (`ull-fleet-sign keygen`).
const OTA_PUBLIC_KEY: [u8; PUBLIC_KEY_LEN] = hex::decode_const(env!(
    "OTA_PUBLIC_KEY",
    "set OTA_PUBLIC_KEY in examples/sht3x-ssd1306/.env (see .env.example)"
));

#[derive(Copy, Clone)]
pub(crate) struct Reading {
    pub temperature_millicelsius: i32,
    pub relative_humidity_hundredths: u16,
}

/// Latest reading for the display.
pub(crate) static DISPLAY_READING: Signal<CriticalSectionRawMutex, Reading> = Signal::new();
/// Latest readings for the fleet server: `temperature_c`, `humidity_pct`.
pub(crate) static METRICS: Metrics<2> = Metrics::new();
/// Tasks the watchdog checks; it reboots the device when one stops reporting.
static WATCHED: [&Pulse; 1] = [&ull_esp_platform::fleet::PULSE];

/// Settings from `.env`, used until settings are saved on the board.
fn built_in_settings() -> Settings {
    let ssid = option_env!("WIFI_SSID").unwrap_or("");
    let password = option_env!("WIFI_PASSWORD").unwrap_or("");
    let server = option_env!("FLEET_BASE_URL").unwrap_or("");
    Settings::new(ssid, password, server).unwrap_or_else(|invalid| {
        if !ssid.is_empty() {
            log::warn!("ignoring the built-in settings: {}", invalid.message());
        }
        Settings::default()
    })
}

pub async fn run(spawner: Spawner) -> Result<(), AppError> {
    runtime::init_default_heap();

    let mut board = Board::init();
    board.start_runtime()?;
    board.start_watchdog(spawner, &WATCHED)?;

    let mut flash = board.take_flash_storage()?;
    let (settings, source) = settings::load(&mut flash, built_in_settings());
    if let Some(reason) = setup::reason(source) {
        log::info!("setup mode: {reason}");
        match board
            .run_setup(spawner, flash, settings, crate::ESP_APP_DESC.version())
            .await? {}
    }
    log::info!(
        "settings: {source:?}, Wi-Fi \"{}\"",
        settings.wifi_ssid.as_str()
    );
    board.watch_setup_button(spawner)?;

    let wifi_config = WifiConfig::new(settings.wifi_ssid.as_str(), settings.wifi_password.as_str());
    let i2c_bus = board.take_i2c0_shared()?;
    let wifi = board.take_wifi_station(spawner, &wifi_config)?;
    board.watch_wifi_for_setup(spawner, wifi.stack())?;

    let sensor =
        sensor::sensor_task(I2cDevice::new(i2c_bus)).map_err(|_| AppError::TaskSpawn("sensor"))?;
    spawner.spawn(sensor);

    let display = display::display_task(I2cDevice::new(i2c_bus))
        .map_err(|_| AppError::TaskSpawn("display"))?;
    spawner.spawn(display);

    match FleetConfig::parse(settings.server_url.as_str()) {
        Some(fleet_config) => {
            let agent = Agent::new(
                fleet_config,
                Firmware::from_app_desc(&crate::ESP_APP_DESC),
                OTA_PUBLIC_KEY,
                flash,
            );
            let fleet =
                fleet::fleet_task(agent, wifi.stack()).map_err(|_| AppError::TaskSpawn("fleet"))?;
            spawner.spawn(fleet);
        }
        None => log::warn!("no fleet server set; running without updates"),
    }

    Ok(())
}
