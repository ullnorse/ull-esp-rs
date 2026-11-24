use thiserror::Error;

#[derive(Debug, Error)]
pub enum BoardError {
    #[error("board resource already taken: {0}")]
    AlreadyTaken(&'static str),
    #[error("i2c init failed")]
    I2c(#[from] esp_hal::i2c::master::ConfigError),
    #[error("spi init failed: {0}")]
    Spi(#[from] esp_hal::spi::master::ConfigError),
    #[error(transparent)]
    Platform(#[from] ull_esp_platform::error::EspError),
    #[error("cannot start a task: {0:?}")]
    Spawn(#[from] embassy_executor::SpawnError),
}
