mod error;
mod i2c;
mod led;
mod wifi;

use crate::pins::{BoardPins, I2c0Pins};

use esp_hal::rng::Rng;

pub use error::BoardError;
pub use i2c::I2c0Parts;
pub use led::StatusLed;
pub use wifi::{WifiParts, WifiStation};

pub struct RuntimeParts {
    pub timg0: esp_hal::peripherals::TIMG0<'static>,
    pub sw_interrupt: esp_hal::peripherals::SW_INTERRUPT<'static>,
}

pub struct Board {
    runtime: Option<RuntimeParts>,
    wifi: Option<WifiParts>,
    i2c0: Option<I2c0Parts>,
    pins: BoardPins,
}

pub struct RawBoardParts {
    pub runtime: RuntimeParts,
    pub wifi: WifiParts,
    pub i2c0: I2c0Parts,
    pub pins: BoardPins,
}

impl RuntimeParts {
    pub fn start(self) {
        ull_esp_platform::runtime::start(self.timg0, self.sw_interrupt);
    }
}

impl Board {
    pub fn init() -> Self {
        Self::init_with_config(ull_esp_platform::runtime::max_clock_config())
    }

    pub fn init_with_config(config: esp_hal::Config) -> Self {
        let peripherals = esp_hal::init(config);
        let esp_hal::peripherals::Peripherals {
            TIMG0: timg0,
            SW_INTERRUPT: sw_interrupt,
            I2C0: i2c0,
            GPIO22: gpio22,
            GPIO21: gpio21,
            GPIO2: gpio2,
            WIFI: wifi,
            ..
        } = peripherals;

        Self {
            runtime: Some(RuntimeParts {
                timg0,
                sw_interrupt,
            }),
            wifi: Some(WifiParts { peripheral: wifi }),
            i2c0: Some(I2c0Parts {
                controller: i2c0,
                pins: I2c0Pins {
                    scl: gpio22,
                    sda: gpio21,
                },
            }),
            pins: BoardPins {
                status_led: Some(gpio2),
            },
        }
    }

    pub fn take_runtime(&mut self) -> Result<RuntimeParts, BoardError> {
        self.runtime
            .take()
            .ok_or(BoardError::AlreadyTaken("runtime"))
    }

    pub fn start_runtime(&mut self) -> Result<(), BoardError> {
        self.take_runtime()?.start();
        Ok(())
    }

    fn wifi_seed() -> u64 {
        let rng = Rng::new();
        ((rng.random() as u64) << 32) | rng.random() as u64
    }

    pub fn into_raw_parts(mut self) -> RawBoardParts {
        RawBoardParts {
            runtime: self
                .runtime
                .take()
                .expect("runtime should exist until into_raw_parts"),
            wifi: self
                .wifi
                .take()
                .expect("wifi should exist until into_raw_parts"),
            i2c0: self
                .i2c0
                .take()
                .expect("i2c0 should exist until into_raw_parts"),
            pins: self.pins,
        }
    }
}
