use thiserror::Error;

#[derive(Debug, Error)]
pub enum EspError {
    #[error("wifi error: {0:?}")]
    Wifi(#[from] esp_radio::wifi::WifiError),
}
