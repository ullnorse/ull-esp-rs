use crate::pins::Spi2Pins;

use esp_hal::{
    Async,
    gpio::{Level, Output, OutputConfig},
    spi::master::{Config, Spi},
};

use super::{Board, BoardError};

pub(crate) struct Spi2Parts {
    pub(crate) controller: esp_hal::peripherals::SPI2<'static>,
    pub(crate) pins: Spi2Pins,
}

impl Spi2Parts {
    fn into_async_with_config(
        self,
        config: Config,
    ) -> Result<Spi<'static, Async>, esp_hal::spi::master::ConfigError> {
        Ok(Spi::new(self.controller, config)?
            .with_sck(self.pins.sck)
            .with_miso(self.pins.miso)
            .with_mosi(self.pins.mosi)
            .into_async())
    }
}

impl Board {
    pub fn take_spi2(&mut self) -> Result<Spi<'static, Async>, BoardError> {
        self.take_spi2_with_config(Config::default())
    }

    pub fn take_spi2_with_config(
        &mut self,
        config: Config,
    ) -> Result<Spi<'static, Async>, BoardError> {
        let parts = self.spi2.take().ok_or(BoardError::AlreadyTaken("spi2"))?;
        Ok(parts.into_async_with_config(config)?)
    }

    pub fn take_spi2_cs(&mut self) -> Result<Output<'static>, BoardError> {
        let pin = self
            .pins
            .spi2_cs
            .take()
            .ok_or(BoardError::AlreadyTaken("spi2_cs"))?;
        Ok(Output::new(pin, Level::High, OutputConfig::default()))
    }
}
