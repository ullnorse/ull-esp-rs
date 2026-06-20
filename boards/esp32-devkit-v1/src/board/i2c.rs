use crate::pins::I2c0Pins;

use esp_hal::{
    Async,
    i2c::master::{Config, ConfigError, I2c},
};

use super::{Board, BoardError};

static SHARED_I2C0_RESOURCES: ull_esp_platform::i2c::SharedI2cResources =
    ull_esp_platform::i2c::SharedI2cResources::new();

pub(crate) struct I2c0Parts {
    pub(crate) controller: esp_hal::peripherals::I2C0<'static>,
    pub(crate) pins: I2c0Pins,
}

impl I2c0Parts {
    fn into_async_with_config(self, config: Config) -> Result<I2c<'static, Async>, ConfigError> {
        ull_esp_platform::i2c::init_i2c_with_config(
            self.controller,
            self.pins.scl,
            self.pins.sda,
            config,
        )
    }
}

impl Board {
    pub fn take_i2c0(&mut self) -> Result<I2c<'static, Async>, BoardError> {
        self.take_i2c0_with_config(Config::default())
    }

    pub fn take_i2c0_with_config(
        &mut self,
        config: Config,
    ) -> Result<I2c<'static, Async>, BoardError> {
        let parts = self.i2c0.take().ok_or(BoardError::AlreadyTaken("i2c0"))?;
        Ok(parts.into_async_with_config(config)?)
    }

    pub fn take_i2c0_shared(
        &mut self,
    ) -> Result<&'static ull_esp_platform::i2c::SharedI2cBus, BoardError> {
        self.take_i2c0_shared_with_config(Config::default())
    }

    pub fn take_i2c0_shared_with_config(
        &mut self,
        config: Config,
    ) -> Result<&'static ull_esp_platform::i2c::SharedI2cBus, BoardError> {
        let i2c = self.take_i2c0_with_config(config)?;
        Ok(SHARED_I2C0_RESOURCES.init(i2c))
    }
}
