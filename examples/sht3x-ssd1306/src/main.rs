#![no_std]
#![no_main]

use embassy_executor::Spawner;

mod app;
mod config;
mod error;
mod reading;
mod tasks;
mod telemetry;

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
async fn main(spawner: Spawner) {
    ull_esp_platform::runtime::init_logger(log::LevelFilter::Info);

    if let Err(err) = app::run(spawner).await {
        log::error!("{err}");
        panic!("fatal startup error")
    }
}
