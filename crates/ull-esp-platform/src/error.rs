use thiserror::Error;

#[derive(Debug, Error)]
pub enum EspError {
    #[error("i2c init failed")]
    I2cInit(#[from] esp_hal::i2c::master::ConfigError),
    #[error("wifi error: {0:?}")]
    Wifi(#[from] esp_radio::wifi::WifiError),
}
