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
    #[error("request buffer too small")]
    RequestBufferTooSmall,
    #[error("tcp read failed")]
    TcpReadFailed,
    #[error("content-length does not fit in u32")]
    InvalidContentLength,
    #[error("missing HTTP content-length")]
    MissingContentLength,
    #[error("missing or invalid x-application-image-sha256 header")]
    InvalidApplicationImageSha256,
    #[error("invalid x-application-image-ed25519 header")]
    InvalidApplicationImageSignature,
    #[error(transparent)]
    Http(#[from] ull_esp_platform::http::HttpClientError),
    #[error("http request failed with status {0}")]
    HttpStatus(u16),
    #[error("failed to create {0} task")]
    TaskSpawn(&'static str),
    #[error(transparent)]
    Board(#[from] ull_esp_board_devkit_v1::BoardError),
    #[error(transparent)]
    Ota(#[from] ull_esp_platform::ota::OtaError),
}
