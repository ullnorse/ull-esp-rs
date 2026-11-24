use esp_hal::Async;
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::peripherals as p;
use esp_hal::time::Rate;

use super::{Board, BoardError};

static SHARED_I2C0_RESOURCES: ull_esp_platform::i2c::SharedI2cResources =
    ull_esp_platform::i2c::SharedI2cResources::new();

pub(crate) struct I2c0Parts {
    pub(crate) controller: p::I2C0<'static>,
    pub(crate) sda: p::GPIO6<'static>,
    pub(crate) scl: p::GPIO7<'static>,
}

impl Board {
    /// 100 kHz: the board's 10k pull-ups are too slow for 400 kHz.
    pub fn i2c0_config() -> Config {
        Config::default().with_frequency(Rate::from_khz(100))
    }

    /// The sensor bus: SHT40 0x44, BMP580 0x46, LIS2DW12 0x19, MAX17048 0x36.
    pub fn take_i2c0(&mut self) -> Result<I2c<'static, Async>, BoardError> {
        let parts = self.i2c0.take().ok_or(BoardError::AlreadyTaken("i2c0"))?;
        Ok(ull_esp_platform::i2c::init_i2c_with_config(
            parts.controller,
            parts.scl,
            parts.sda,
            Self::i2c0_config(),
        )?)
    }

    /// The sensor bus behind a mutex, for one driver per task.
    pub fn take_i2c0_shared(
        &mut self,
    ) -> Result<&'static ull_esp_platform::i2c::SharedI2cBus, BoardError> {
        let i2c = self.take_i2c0()?;
        Ok(SHARED_I2C0_RESOURCES.init(i2c))
    }
}
