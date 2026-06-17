use esp_hal::Async;
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::spi::master::{Config, ConfigError, Instance, Spi};

pub fn init_spi<'d>(
    spi: impl Instance + 'd,
    sck: impl PeripheralOutput<'d>,
    miso: impl PeripheralInput<'d>,
    mosi: impl PeripheralOutput<'d>,
) -> Result<Spi<'d, Async>, ConfigError> {
    init_spi_with_config(spi, sck, miso, mosi, Config::default())
}

pub fn init_spi_with_config<'d>(
    spi: impl Instance + 'd,
    sck: impl PeripheralOutput<'d>,
    miso: impl PeripheralInput<'d>,
    mosi: impl PeripheralOutput<'d>,
    config: Config,
) -> Result<Spi<'d, Async>, ConfigError> {
    Ok(Spi::new(spi, config)?
        .with_sck(sck)
        .with_miso(miso)
        .with_mosi(mosi)
        .into_async())
}
