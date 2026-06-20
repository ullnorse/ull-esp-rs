use crate::pins::Uart2Pins;

use esp_hal::{
    Async,
    uart::{Config, Uart},
};

use super::{Board, BoardError};

pub(crate) struct Uart2Parts {
    pub(crate) controller: esp_hal::peripherals::UART2<'static>,
    pub(crate) pins: Uart2Pins,
}

impl Uart2Parts {
    fn into_async_with_config(
        self,
        config: Config,
    ) -> Result<Uart<'static, Async>, esp_hal::uart::ConfigError> {
        ull_esp_platform::uart::init_uart_with_config(
            self.controller,
            self.pins.tx,
            self.pins.rx,
            config,
        )
    }
}

impl Board {
    pub fn take_uart2(&mut self) -> Result<Uart<'static, Async>, BoardError> {
        self.take_uart2_with_config(Config::default())
    }

    pub fn take_uart2_with_config(
        &mut self,
        config: Config,
    ) -> Result<Uart<'static, Async>, BoardError> {
        let parts = self.uart2.take().ok_or(BoardError::AlreadyTaken("uart2"))?;
        Ok(parts.into_async_with_config(config)?)
    }
}
