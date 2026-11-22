use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid FLEET_BASE_URL, expected http://<IPv4>:<port>")]
    InvalidFleetBaseUrl,
    #[error("failed to create {0} task")]
    TaskSpawn(&'static str),
    #[error(transparent)]
    Board(#[from] ull_esp_board_devkit_v1::BoardError),
}
