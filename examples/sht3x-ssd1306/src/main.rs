#![no_std]
#![no_main]

use embassy_executor::Spawner;

mod app;
mod error;
mod tasks;

ull_esp_platform::firmware_app_desc!();

ull_esp_platform::panic_handler!();

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
