use crate::pins::I2c0Pins;

use super::{Board, BoardError};

static SHARED_I2C0_RESOURCES: ull_esp_platform::SharedI2cResources =
    ull_esp_platform::SharedI2cResources::new();

pub struct I2c0Parts {
    pub controller: esp_hal::peripherals::I2C0<'static>,
    pub pins: I2c0Pins,
}

impl I2c0Parts {
    pub fn into_async(
        self,
    ) -> Result<ull_esp_platform::SharedI2c, esp_hal::i2c::master::ConfigError> {
        self.into_async_with_config(ull_esp_platform::I2cConfig::default())
    }

    pub fn into_async_with_config(
        self,
        config: ull_esp_platform::I2cConfig,
    ) -> Result<ull_esp_platform::SharedI2c, esp_hal::i2c::master::ConfigError> {
        ull_esp_platform::i2c::init_i2c_with_config(
            self.controller,
            self.pins.scl,
            self.pins.sda,
            config,
        )
    }

    pub fn into_shared_bus(
        self,
        resources: &'static ull_esp_platform::SharedI2cResources,
    ) -> Result<&'static ull_esp_platform::SharedI2cBus, esp_hal::i2c::master::ConfigError> {
        self.into_shared_bus_with_config(resources, ull_esp_platform::I2cConfig::default())
    }

    pub fn into_shared_bus_with_config(
        self,
        resources: &'static ull_esp_platform::SharedI2cResources,
        config: ull_esp_platform::I2cConfig,
    ) -> Result<&'static ull_esp_platform::SharedI2cBus, esp_hal::i2c::master::ConfigError> {
        let i2c = self.into_async_with_config(config)?;
        Ok(resources.init(i2c))
    }
}

impl Board {
    pub fn take_i2c0_parts(&mut self) -> Result<I2c0Parts, BoardError> {
        self.i2c0.take().ok_or(BoardError::AlreadyTaken("i2c0"))
    }

    pub fn take_shared_i2c0(
        &mut self,
    ) -> Result<&'static ull_esp_platform::SharedI2cBus, BoardError> {
        self.take_shared_i2c0_with_config(ull_esp_platform::I2cConfig::default())
    }

    pub fn take_shared_i2c0_with_config(
        &mut self,
        config: ull_esp_platform::I2cConfig,
    ) -> Result<&'static ull_esp_platform::SharedI2cBus, BoardError> {
        self.take_shared_i2c0_with_resources_and_config(&SHARED_I2C0_RESOURCES, config)
    }

    pub fn take_shared_i2c0_with_resources(
        &mut self,
        resources: &'static ull_esp_platform::SharedI2cResources,
    ) -> Result<&'static ull_esp_platform::SharedI2cBus, BoardError> {
        self.take_shared_i2c0_with_resources_and_config(
            resources,
            ull_esp_platform::I2cConfig::default(),
        )
    }

    pub fn take_shared_i2c0_with_resources_and_config(
        &mut self,
        resources: &'static ull_esp_platform::SharedI2cResources,
        config: ull_esp_platform::I2cConfig,
    ) -> Result<&'static ull_esp_platform::SharedI2cBus, BoardError> {
        let i2c = self.take_i2c0_parts()?.into_async_with_config(config)?;
        Ok(resources.init(i2c))
    }
}
