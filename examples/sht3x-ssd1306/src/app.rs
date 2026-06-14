use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::rng::Rng;
use esp_radio::wifi::WifiController;
use ull_esp_board_devkit_v1::Board;
use ull_esp_platform::{SharedI2cBus, SharedI2cResources, WifiRunner, WifiStackResources};
use ull_esp_platform::{i2c, runtime, wifi};

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

    let Board {
        runtime: runtime_parts,
        wifi: wifi_device,
        i2c0,
        pins: _pins,
    } = Board::init(runtime::max_clock_config());

    runtime::start(runtime_parts.timg0, runtime_parts.sw_interrupt);

    let i2c = i2c::init_i2c(i2c0.controller, i2c0.pins.scl, i2c0.pins.sda)
        .map_err(ull_esp_platform::EspError::from)?;
    let i2c_bus = APP_RESOURCES.i2c_bus.init(i2c);

    let rng = Rng::new();
    let seed = ((rng.random() as u64) << 32) | rng.random() as u64;
    let mut wifi_parts = wifi::init_station_dhcp(wifi_device, seed, &APP_RESOURCES.wifi_stack)?;
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
