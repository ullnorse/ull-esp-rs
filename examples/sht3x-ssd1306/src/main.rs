#![no_std]
#![no_main]

use embassy_executor::Spawner;

mod app;
mod tasks;
ull_esp_platform::firmware_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    ull_esp_platform::runtime::run_app(spawner, app::run).await
}
