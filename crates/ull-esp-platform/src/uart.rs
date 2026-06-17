use esp_hal::Async;
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::uart::{Config, ConfigError, Instance, Uart};

pub fn init_uart<'d>(
    uart: impl Instance + 'd,
    tx: impl PeripheralOutput<'d>,
    rx: impl PeripheralInput<'d>,
) -> Result<Uart<'d, Async>, ConfigError> {
    init_uart_with_config(uart, tx, rx, Config::default())
}

pub fn init_uart_with_config<'d>(
    uart: impl Instance + 'd,
    tx: impl PeripheralOutput<'d>,
    rx: impl PeripheralInput<'d>,
    config: Config,
) -> Result<Uart<'d, Async>, ConfigError> {
    Ok(Uart::new(uart, config)?
        .with_tx(tx)
        .with_rx(rx)
        .into_async())
}
