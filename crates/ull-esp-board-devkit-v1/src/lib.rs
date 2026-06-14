#![no_std]

mod board;
mod pins;

pub use board::{
    Board, BoardError, I2c0Parts, RawBoardParts, RuntimeParts, StatusLed, WifiParts,
    WifiStation,
};
pub use pins::{BoardPins, I2c0Pins, I2c0SclPin, I2c0SdaPin, StatusLedPin};
