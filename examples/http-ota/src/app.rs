//! The smallest fleet device: checks in with the server, installs signed
//! updates, and blinks the status LED. No sensors.

use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use ull_esp_board_devkit_v1::{Board, StatusLed};
use ull_esp_platform::fleet::{Agent, Firmware, Metrics};
use ull_esp_platform::runtime;

use crate::config::{AppConfig, OTA_PUBLIC_KEY};
use crate::error::AppError;

const STATUS_LED_BLINK_PERIOD: Duration = Duration::from_millis(5000);

/// This device reports no metrics; the agent still sends heartbeats.
static METRICS: Metrics<1> = Metrics::new();

pub async fn run(spawner: Spawner) -> Result<(), AppError> {
    runtime::init_default_heap();
    let config = AppConfig::from_env()?;

    let mut board = Board::init();
    board.start_runtime()?;

    let status_led = board.take_status_led()?;
    let flash = board.take_flash_storage()?;
    let wifi = board.take_wifi_station(spawner, &config.wifi)?;
    let agent = Agent::new(
        config.fleet,
        Firmware::from_app_desc(&crate::ESP_APP_DESC),
        OTA_PUBLIC_KEY,
        flash,
    );

    spawner.spawn(status_led_task(status_led).map_err(|_| AppError::TaskSpawn("status-led"))?);
    spawner.spawn(fleet_task(agent, wifi.stack()).map_err(|_| AppError::TaskSpawn("fleet"))?);
    Ok(())
}

#[embassy_executor::task]
async fn fleet_task(agent: Agent, stack: Stack<'static>) {
    agent.run(stack, &METRICS).await
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
