pub type StatusLedPin = esp_hal::peripherals::GPIO2<'static>;
pub type I2c0SclPin = esp_hal::peripherals::GPIO22<'static>;
pub type I2c0SdaPin = esp_hal::peripherals::GPIO21<'static>;

pub struct I2c0Pins {
    pub scl: I2c0SclPin,
    pub sda: I2c0SdaPin,
}

pub struct BoardPins {
    pub status_led: StatusLedPin,
}
