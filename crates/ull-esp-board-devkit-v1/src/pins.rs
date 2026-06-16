pub(crate) type StatusLedPin = esp_hal::peripherals::GPIO2<'static>;
pub(crate) type I2c0SclPin = esp_hal::peripherals::GPIO22<'static>;
pub(crate) type I2c0SdaPin = esp_hal::peripherals::GPIO21<'static>;

pub(crate) struct I2c0Pins {
    pub(crate) scl: I2c0SclPin,
    pub(crate) sda: I2c0SdaPin,
}

pub(crate) struct BoardPins {
    pub(crate) status_led: Option<StatusLedPin>,
}
