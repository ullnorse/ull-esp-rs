#![no_std]
#![no_main]

use embassy_executor::Spawner;
use ull_esp_board_devkit_v1::Board;

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log::error!("PANIC: {:?}", info);
    loop {}
}

#[unsafe(no_mangle)]
pub extern "Rust" fn _esp_println_timestamp() -> u64 {
    esp_hal::time::Instant::now()
        .duration_since_epoch()
        .as_millis()
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    ull_esp_platform::runtime::init_logger(log::LevelFilter::Info);

    let mut board = Board::init();
    board.start_runtime().unwrap();

    let mut led = board.take_status_led().unwrap();

    loop {
        led.toggle();
        log::info!("status led toggled");
        board.sleep_ms(500).await;
    }
}
