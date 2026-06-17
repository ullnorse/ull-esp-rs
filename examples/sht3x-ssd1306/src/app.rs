use core::net::Ipv4Addr;

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_net::Ipv4Address;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, TrySendError};
use embassy_sync::signal::Signal;
use ull_esp_board_devkit_v1::Board;
use ull_esp_platform::{config::WifiConfig, i2c::SharedI2cBus, runtime};

use crate::error::AppError;
use crate::tasks::{display, http, sensor};

pub(crate) struct AppConfig {
    wifi: WifiConfig<'static>,
    readings: ReadingsConfig,
}

#[derive(Copy, Clone)]
pub(crate) struct ReadingsConfig {
    pub server_addr: &'static str,
    pub path: &'static str,
    pub server_ip: Ipv4Address,
    pub port: u16,
}

#[derive(Copy, Clone)]
pub(crate) struct Reading {
    pub temperature_millicelsius: i32,
    pub relative_humidity_hundredths: u16,
}

pub(crate) struct AppResources {
    pub display_reading: Signal<CriticalSectionRawMutex, Reading>,
    pub publish_readings: Channel<CriticalSectionRawMutex, Reading, 8>,
}

impl AppResources {
    const fn new() -> Self {
        Self {
            display_reading: Signal::new(),
            publish_readings: Channel::new(),
        }
    }

    pub fn enqueue_publish_reading(&self, reading: Reading) {
        if let Err(TrySendError::Full(reading)) = self.publish_readings.try_send(reading) {
            let _ = self.publish_readings.try_receive();
            let _ = self.publish_readings.try_send(reading);
        }
    }
}

pub(crate) static APP_RESOURCES: AppResources = AppResources::new();

impl AppConfig {
    fn from_env() -> Result<Self, AppError> {
        Ok(Self {
            wifi: WifiConfig::new(env!("WIFI_SSID"), env!("WIFI_PASSWORD")),
            readings: ReadingsConfig::from_env()?,
        })
    }
}

impl ReadingsConfig {
    fn from_env() -> Result<Self, AppError> {
        let server_addr = option_env!("READINGS_ADDR")
            .or(option_env!("READINGS_HOST"))
            .ok_or(AppError::MissingReadingsAddr)?;

        Ok(Self {
            server_addr,
            path: match option_env!("READINGS_PATH") {
                Some(path) if !path.is_empty() => path,
                _ => "/",
            },
            server_ip: parse_ipv4(server_addr).ok_or(AppError::InvalidReadingsAddr)?,
            port: option_env!("READINGS_PORT")
                .unwrap_or("3000")
                .parse()
                .map_err(|_| AppError::InvalidReadingsPort)?,
        })
    }
}

pub async fn run(spawner: Spawner) -> Result<(), AppError> {
    runtime::init_default_heap();
    let config = AppConfig::from_env()?;

    let mut board = Board::init();
    board.start_runtime()?;

    let i2c_bus = board.take_i2c0_shared()?;
    let wifi = board.take_wifi_station(spawner, &config.wifi)?;

    spawn_tasks(spawner, i2c_bus, wifi.stack(), config.readings)
}

fn spawn_tasks(
    spawner: Spawner,
    i2c_bus: &'static SharedI2cBus,
    stack: Stack<'static>,
    readings_config: ReadingsConfig,
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

fn parse_ipv4(value: &str) -> Option<Ipv4Address> {
    let parsed: Ipv4Addr = value.parse().ok()?;
    let [a, b, c, d] = parsed.octets();
    Some(Ipv4Address::new(a, b, c, d))
}
