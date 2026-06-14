use crate::pins::{BoardPins, I2c0Pins};

pub struct RuntimeParts {
    pub timg0: esp_hal::peripherals::TIMG0<'static>,
    pub sw_interrupt: esp_hal::peripherals::SW_INTERRUPT<'static>,
}

pub struct Board {
    pub runtime: RuntimeParts,
    pub wifi: esp_hal::peripherals::WIFI<'static>,
    pub i2c0: I2c0Parts,
    pub pins: BoardPins,
}

pub struct I2c0Parts {
    pub controller: esp_hal::peripherals::I2C0<'static>,
    pub pins: I2c0Pins,
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
            runtime: RuntimeParts {
                timg0,
                sw_interrupt,
            },
            wifi,
            i2c0: I2c0Parts {
                controller: i2c0,
                pins: I2c0Pins {
                    scl: gpio22,
                    sda: gpio21,
                },
            },
            pins: BoardPins { status_led: gpio2 },
        }
    }
}
