use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use ull_esp_board_devkit_v1::Board;
use ull_esp_platform::{SharedI2cBus, runtime};

use crate::config;
use crate::error::AppError;
use crate::reading::Reading;
use crate::tasks::{display, http, sensor};

pub(crate) struct AppResources {
    pub display_reading: Signal<CriticalSectionRawMutex, Reading>,
    pub wifi_reading: Signal<CriticalSectionRawMutex, Reading>,
}

impl AppResources {
    const fn new() -> Self {
        Self {
            display_reading: Signal::new(),
            wifi_reading: Signal::new(),
        }
    }
}

pub(crate) static APP_RESOURCES: AppResources = AppResources::new();

pub async fn run(spawner: Spawner) -> Result<(), AppError> {
    runtime::init_default_heap();

    let mut board = Board::init();
    board.start_runtime()?;

    let i2c_bus = board.take_i2c0_shared()?;
    let wifi = board.take_wifi_station(spawner, &config::wifi_config())?;
    let readings_config = config::readings_config()?;

    spawn_tasks(spawner, i2c_bus, wifi.stack(), readings_config)
}

fn spawn_tasks(
    spawner: Spawner,
    i2c_bus: &'static SharedI2cBus,
    stack: Stack<'static>,
    readings_config: config::ReadingsConfig,
) -> Result<(), AppError> {
    let sensor =
        sensor::sensor_task(I2cDevice::new(i2c_bus)).map_err(|_| AppError::TaskSpawn("sensor"))?;
    spawner.spawn(sensor);

    let display = display::display_task(I2cDevice::new(i2c_bus))
        .map_err(|_| AppError::TaskSpawn("display"))?;
    spawner.spawn(display);

    let http = http::http_task(stack, readings_config).map_err(|_| AppError::TaskSpawn("http"))?;
    spawner.spawn(http);

    Ok(())
}
