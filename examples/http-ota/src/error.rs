use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("missing WIFI_SSID")]
    MissingWifiSsid,
    #[error("missing WIFI_PASSWORD")]
    MissingWifiPassword,
    #[error("missing OTA_TOKEN")]
    MissingOtaToken,
    #[error("invalid OTA_PORT")]
    InvalidOtaPort,
    #[error("failed to create {0} task")]
    TaskSpawn(&'static str),
    #[error(transparent)]
    Board(#[from] ull_esp_board_devkit_v1::BoardError),
    #[error(transparent)]
    Ota(#[from] ull_esp_platform::ota::OtaError),
}
