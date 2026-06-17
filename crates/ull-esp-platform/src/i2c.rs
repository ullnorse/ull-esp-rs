use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::Async;
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2c::master::{Config, ConfigError, I2c, Instance};
use static_cell::StaticCell;

pub type SharedI2c = I2c<'static, Async>;
pub type SharedI2cBus = Mutex<CriticalSectionRawMutex, SharedI2c>;

pub struct SharedI2cResources {
    bus: StaticCell<SharedI2cBus>,
}

impl SharedI2cResources {
    pub const fn new() -> Self {
        Self {
            bus: StaticCell::new(),
        }
    }

    pub fn init(&'static self, i2c: SharedI2c) -> &'static SharedI2cBus {
        self.bus.init(Mutex::new(i2c))
    }
}

impl Default for SharedI2cResources {
    fn default() -> Self {
        Self::new()
    }
}

pub fn init_i2c<'d>(
    i2c: impl Instance + 'd,
    scl: impl PeripheralOutput<'d> + PeripheralInput<'d>,
    sda: impl PeripheralOutput<'d> + PeripheralInput<'d>,
) -> Result<I2c<'d, Async>, ConfigError> {
    init_i2c_with_config(i2c, scl, sda, Config::default())
}

pub fn init_i2c_with_config<'d>(
    i2c: impl Instance + 'd,
    scl: impl PeripheralOutput<'d> + PeripheralInput<'d>,
    sda: impl PeripheralOutput<'d> + PeripheralInput<'d>,
    config: Config,
) -> Result<I2c<'d, Async>, ConfigError> {
    Ok(I2c::new(i2c, config)?
        .with_scl(scl)
        .with_sda(sda)
        .into_async())
}
