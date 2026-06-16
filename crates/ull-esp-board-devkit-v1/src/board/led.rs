use crate::pins::StatusLedPin;

use esp_hal::gpio::{Level, Output, OutputConfig};

use super::{Board, BoardError};

pub struct StatusLed {
    pin: Output<'static>,
}

impl StatusLed {
    pub(crate) fn new(pin: StatusLedPin) -> Self {
        Self {
            pin: Output::new(pin, Self::off_level(), OutputConfig::default()),
        }
    }

    pub fn on(&mut self) {
        self.pin.set_level(Self::on_level());
    }

    pub fn off(&mut self) {
        self.pin.set_level(Self::off_level());
    }

    pub fn toggle(&mut self) {
        self.pin.toggle();
    }

    const fn on_level() -> Level {
        Level::High
    }

    const fn off_level() -> Level {
        Level::Low
    }
}

impl Board {
    pub fn take_status_led_pin(&mut self) -> Result<StatusLedPin, BoardError> {
        self.pins
            .status_led
            .take()
            .ok_or(BoardError::AlreadyTaken("status_led"))
    }

    pub fn take_status_led(&mut self) -> Result<StatusLed, BoardError> {
        Ok(StatusLed::new(self.take_status_led_pin()?))
    }
}
