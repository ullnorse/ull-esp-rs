use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use ull_esp_board_devkit_v1::{Board, BoardError};
use ull_esp_platform::config::WifiConfig;
use ull_esp_platform::fleet::{self, FleetMetrics};
use ull_esp_platform::watchdog::Pulse;
use ull_esp_platform::{settings, setup};

use crate::ESP_APP_DESC;
use crate::tasks::{display, sensor};

#[derive(Copy, Clone)]
pub(crate) struct Reading {
    pub temperature_millicelsius: i32,
    pub relative_humidity_hundredths: u16,
}

/// Latest reading for the display.
pub(crate) static DISPLAY_READING: Signal<CriticalSectionRawMutex, Reading> = Signal::new();
/// Latest readings for the fleet server: `temperature_c`, `humidity_pct`.
pub(crate) static METRICS: FleetMetrics = FleetMetrics::new();
/// Tasks the watchdog checks; it reboots the device when one stops reporting.
static WATCHED: [&Pulse; 1] = [&fleet::PULSE];

pub async fn run(spawner: Spawner) -> Result<(), BoardError> {
    let mut board = Board::init();
    board.start_watchdog(spawner, &WATCHED)?;

    // Settings saved on the board, or setup mode from a phone when there are none.
    let mut flash = board.take_flash_storage()?;
    let (settings, source) = settings::load(&mut flash, settings::built_in());
    if let Some(reason) = setup::reason(source) {
        log::info!("setup mode: {reason}");
        match board
            .run_setup(spawner, flash, settings, ESP_APP_DESC.version())
            .await? {}
    }
    board.watch_setup_button(spawner)?;

    let wifi_config = WifiConfig::new(settings.wifi_ssid.as_str(), settings.wifi_password.as_str());
    let wifi = board.take_wifi_station(spawner, &wifi_config)?;
    board.watch_wifi_for_setup(spawner, wifi.stack())?;

    // Check-ins, metrics and signed updates.
    fleet::start(
        spawner,
        wifi.stack(),
        flash,
        settings.server_url.as_str(),
        &ESP_APP_DESC,
        &METRICS,
    )?;

    let i2c_bus = board.take_i2c0_shared()?;
    spawner.spawn(sensor::sensor_task(I2cDevice::new(i2c_bus))?);
    spawner.spawn(display::display_task(I2cDevice::new(i2c_bus))?);
    Ok(())
}
