pub(crate) type StatusLedPin = esp_hal::peripherals::GPIO2<'static>;
pub(crate) type I2c0SclPin = esp_hal::peripherals::GPIO22<'static>;
pub(crate) type I2c0SdaPin = esp_hal::peripherals::GPIO21<'static>;
pub(crate) type Spi2SckPin = esp_hal::peripherals::GPIO18<'static>;
pub(crate) type Spi2MisoPin = esp_hal::peripherals::GPIO19<'static>;
pub(crate) type Spi2MosiPin = esp_hal::peripherals::GPIO23<'static>;
pub(crate) type Spi2CsPin = esp_hal::peripherals::GPIO5<'static>;
pub(crate) type Uart2TxPin = esp_hal::peripherals::GPIO17<'static>;
pub(crate) type Uart2RxPin = esp_hal::peripherals::GPIO16<'static>;

pub(crate) struct I2c0Pins {
    pub(crate) scl: I2c0SclPin,
    pub(crate) sda: I2c0SdaPin,
}

pub(crate) struct Spi2Pins {
    pub(crate) sck: Spi2SckPin,
    pub(crate) miso: Spi2MisoPin,
    pub(crate) mosi: Spi2MosiPin,
}

pub(crate) struct Uart2Pins {
    pub(crate) tx: Uart2TxPin,
    pub(crate) rx: Uart2RxPin,
}

pub(crate) struct BoardPins {
    pub(crate) status_led: Option<StatusLedPin>,
    pub(crate) spi2_cs: Option<Spi2CsPin>,
}
