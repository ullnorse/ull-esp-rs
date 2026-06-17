use thiserror::Error;

#[derive(Debug, Error)]
pub enum BoardError {
    #[error("board resource already taken: {0}")]
    AlreadyTaken(&'static str),
    #[error("i2c init failed")]
    I2c(#[from] esp_hal::i2c::master::ConfigError),
    #[error("spi init failed: {0}")]
    Spi(#[from] esp_hal::spi::master::ConfigError),
    #[error("uart init failed: {0}")]
    Uart(#[from] esp_hal::uart::ConfigError),
    #[error("wifi error: {0}")]
    Wifi(#[from] ull_esp_platform::error::EspError),
    #[error("failed to spawn {0} task")]
    TaskSpawn(&'static str),
}
