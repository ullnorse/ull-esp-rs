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
    #[error("response buffer too small")]
    ResponseBufferTooSmall,
    #[error("tcp connect failed")]
    TcpConnectFailed,
    #[error("tcp read failed")]
    TcpReadFailed,
    #[error("tcp write failed")]
    TcpWriteFailed,
    #[error("tcp flush failed")]
    TcpFlushFailed,
    #[error("invalid HTTP response")]
    InvalidHttpResponse,
    #[error("invalid HTTP content-length")]
    InvalidContentLength,
    #[error("missing HTTP content-length")]
    MissingContentLength,
    #[error("invalid application image sha256")]
    InvalidApplicationImageSha256,
    #[error("http request failed with status {0}")]
    HttpStatus(u16),
    #[error("failed to create {0} task")]
    TaskSpawn(&'static str),
    #[error(transparent)]
    Board(#[from] ull_esp_board_devkit_v1::BoardError),
    #[error(transparent)]
    Ota(#[from] ull_esp_platform::ota::OtaError),
}
