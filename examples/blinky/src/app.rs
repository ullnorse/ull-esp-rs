//! Blinks the status LED. No network, no updates.

use embassy_executor::Spawner;
use embassy_time::Timer;
use ull_esp_board_devkit_v1::{Board, BoardError};

pub async fn run(_spawner: Spawner) -> Result<(), BoardError> {
    let mut board = Board::init();
    let mut led = board.take_status_led()?;
    loop {
        led.toggle();
        log::info!("status led toggled");
        Timer::after_millis(500).await;
    }
}
