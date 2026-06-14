use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::rng::Rng;
use esp_radio::wifi::WifiController;
use ull_esp_board_devkit_v1::Board;
use ull_esp_platform::{
    SharedI2cBus, SharedI2cResources, StationNetworkConfig, WifiRunner, WifiStackResources,
};
use ull_esp_platform::{runtime, wifi};

use crate::config;
use crate::error::AppError;
use crate::reading::Reading;
use crate::tasks::{display, http, sensor};

pub(crate) struct AppResources {
    pub i2c_bus: SharedI2cResources,
    pub display_reading: Signal<CriticalSectionRawMutex, Reading>,
    pub wifi_reading: Signal<CriticalSectionRawMutex, Reading>,
    pub wifi_stack: WifiStackResources<3>,
}

impl AppResources {
    const fn new() -> Self {
        Self {
            i2c_bus: SharedI2cResources::new(),
            display_reading: Signal::new(),
            wifi_reading: Signal::new(),
            wifi_stack: WifiStackResources::new(),
        }
    }
}

pub(crate) static APP_RESOURCES: AppResources = AppResources::new();

pub async fn run(spawner: Spawner) -> Result<(), AppError> {
    runtime::init_default_heap();

    let mut board = Board::init();

    let runtime_parts = board.take_runtime().expect("board runtime available during startup");
    runtime_parts.start();

    let i2c_bus = board
        .take_i2c0_parts()
        .expect("board i2c0 available during startup")
        .into_shared_bus(&APP_RESOURCES.i2c_bus)
        .map_err(ull_esp_platform::EspError::from)?;

    let rng = Rng::new();
    let seed = ((rng.random() as u64) << 32) | rng.random() as u64;
    let mut wifi_parts = board
        .take_wifi_parts()
        .expect("board wifi available during startup")
        .into_station(seed, &APP_RESOURCES.wifi_stack, StationNetworkConfig::dhcpv4())?;
    wifi::configure(&mut wifi_parts.controller, &config::wifi_config())?;
    let readings_config = config::readings_config()?;

    spawn_tasks(
        spawner,
        i2c_bus,
        wifi_parts.controller,
        wifi_parts.stack,
        wifi_parts.runner,
        readings_config,
    )
}

fn spawn_tasks(
    spawner: Spawner,
    i2c_bus: &'static SharedI2cBus,
    controller: WifiController<'static>,
    stack: Stack<'static>,
    runner: WifiRunner,
    readings_config: config::ReadingsConfig,
) -> Result<(), AppError> {
    let sensor =
        sensor::sensor_task(I2cDevice::new(i2c_bus)).map_err(|_| AppError::TaskSpawn("sensor"))?;
    spawner.spawn(sensor);

    let display = display::display_task(I2cDevice::new(i2c_bus))
        .map_err(|_| AppError::TaskSpawn("display"))?;
    spawner.spawn(display);

    let wifi = wifi::connection_task(controller).map_err(|_| AppError::TaskSpawn("wifi"))?;
    spawner.spawn(wifi);

    let network = wifi::runner_task(runner).map_err(|_| AppError::TaskSpawn("net"))?;
    spawner.spawn(network);

    let http = http::http_task(stack, readings_config).map_err(|_| AppError::TaskSpawn("http"))?;
    spawner.spawn(http);

    Ok(())
}
