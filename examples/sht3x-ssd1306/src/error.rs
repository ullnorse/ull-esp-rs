use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("body buffer too small")]
    BodyBufferTooSmall,
    #[error("request buffer too small")]
    RequestBufferTooSmall,
    #[error("tcp connect failed")]
    TcpConnectFailed,
    #[error("tcp write failed")]
    TcpWriteFailed,
    #[error("tcp flush failed")]
    TcpFlushFailed,
    #[error("invalid HTTP response")]
    InvalidHttpResponse,
    #[error("http request failed with status {0}")]
    HttpStatus(u16),
    #[error("invalid READINGS_PORT")]
    InvalidReadingsPort,
    #[error("invalid READINGS_HOST, expected IPv4 address like 192.168.1.10")]
    InvalidReadingsHost,
    #[error("failed to create {0} task")]
    TaskSpawn(&'static str),
    #[error(transparent)]
    Board(#[from] ull_esp_board_devkit_v1::BoardError),
    #[error(transparent)]
    Esp(#[from] ull_esp_platform::error::EspError),
}
