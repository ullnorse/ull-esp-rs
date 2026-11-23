//! The smallest fleet device: checks in with the server, installs signed
//! updates, and blinks the status LED. No sensors and no setup mode: Wi-Fi
//! and the server come from the settings saved on the board, or else from
//! `~/.config/ull-esp/device.env`.

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use ull_esp_board_devkit_v1::{Board, BoardError, StatusLed};
use ull_esp_platform::config::WifiConfig;
use ull_esp_platform::fleet::{self, FleetMetrics};
use ull_esp_platform::settings;
use ull_esp_platform::watchdog::Pulse;

use crate::ESP_APP_DESC;

const STATUS_LED_BLINK_PERIOD: Duration = Duration::from_millis(5000);

/// This device reports no metrics; the agent still sends heartbeats.
static METRICS: FleetMetrics = FleetMetrics::new();
/// Tasks the watchdog checks; it reboots the device when one stops reporting.
static WATCHED: [&Pulse; 1] = [&fleet::PULSE];

pub async fn run(spawner: Spawner) -> Result<(), BoardError> {
    let mut board = Board::init();
    board.start_watchdog(spawner, &WATCHED)?;

    let mut flash = board.take_flash_storage()?;
    let (settings, _) = settings::load(&mut flash, settings::built_in());
    let wifi_config = WifiConfig::new(settings.wifi_ssid.as_str(), settings.wifi_password.as_str());
    let wifi = board.take_wifi_station(spawner, &wifi_config)?;
    fleet::start(
        spawner,
        wifi.stack(),
        flash,
        settings.server_url.as_str(),
        &ESP_APP_DESC,
        &METRICS,
    )?;

    spawner.spawn(status_led_task(board.take_status_led()?)?);
    Ok(())
}

#[embassy_executor::task]
async fn status_led_task(mut status_led: StatusLed) {
    let half_period = STATUS_LED_BLINK_PERIOD / 2;
    loop {
        status_led.on();
        Timer::after(half_period).await;
        status_led.off();
        Timer::after(half_period).await;
    }
}
