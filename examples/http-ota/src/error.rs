use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("missing WIFI_SSID")]
    MissingWifiSsid,
    #[error("missing WIFI_PASSWORD")]
    MissingWifiPassword,
    #[error("missing FLEET_BASE_URL")]
    MissingFleetBaseUrl,
    #[error("invalid FLEET_BASE_URL, expected http://192.168.1.10:3000")]
    InvalidFleetBaseUrl,
    #[error("failed to create {0} task")]
    TaskSpawn(&'static str),
    #[error(transparent)]
    Board(#[from] ull_esp_board_devkit_v1::BoardError),
}
