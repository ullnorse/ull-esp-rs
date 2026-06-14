use crate::pins::{BoardPins, I2c0Pins, StatusLedPin};

use thiserror::Error;

pub struct RuntimeParts {
    pub timg0: esp_hal::peripherals::TIMG0<'static>,
    pub sw_interrupt: esp_hal::peripherals::SW_INTERRUPT<'static>,
}

#[derive(Debug, Error)]
pub enum BoardError {
    #[error("board resource already taken: {0}")]
    AlreadyTaken(&'static str),
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

pub struct I2c0Parts {
    pub controller: esp_hal::peripherals::I2C0<'static>,
    pub pins: I2c0Pins,
}

pub struct WifiParts {
    pub peripheral: esp_hal::peripherals::WIFI<'static>,
}

impl RuntimeParts {
    pub fn start(self) {
        ull_esp_platform::runtime::start(self.timg0, self.sw_interrupt);
    }
}

impl I2c0Parts {
    pub fn into_async(
        self,
    ) -> Result<ull_esp_platform::SharedI2c, esp_hal::i2c::master::ConfigError> {
        self.into_async_with_config(ull_esp_platform::I2cConfig::default())
    }

    pub fn into_async_with_config(
        self,
        config: ull_esp_platform::I2cConfig,
    ) -> Result<ull_esp_platform::SharedI2c, esp_hal::i2c::master::ConfigError> {
        ull_esp_platform::i2c::init_i2c_with_config(
            self.controller,
            self.pins.scl,
            self.pins.sda,
            config,
        )
    }

    pub fn into_shared_bus(
        self,
        resources: &'static ull_esp_platform::SharedI2cResources,
    ) -> Result<&'static ull_esp_platform::SharedI2cBus, esp_hal::i2c::master::ConfigError> {
        self.into_shared_bus_with_config(resources, ull_esp_platform::I2cConfig::default())
    }

    pub fn into_shared_bus_with_config(
        self,
        resources: &'static ull_esp_platform::SharedI2cResources,
        config: ull_esp_platform::I2cConfig,
    ) -> Result<&'static ull_esp_platform::SharedI2cBus, esp_hal::i2c::master::ConfigError> {
        let i2c = self.into_async_with_config(config)?;
        Ok(resources.init(i2c))
    }
}

impl WifiParts {
    pub fn into_station<const SOCKETS: usize>(
        self,
        seed: u64,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
        config: ull_esp_platform::StationNetworkConfig,
    ) -> Result<ull_esp_platform::WifiStackParts, ull_esp_platform::EspError> {
        ull_esp_platform::wifi::init_station(self.peripheral, seed, resources, config)
    }

    pub fn into_station_dhcp<const SOCKETS: usize>(
        self,
        seed: u64,
        resources: &'static ull_esp_platform::WifiStackResources<SOCKETS>,
    ) -> Result<ull_esp_platform::WifiStackParts, ull_esp_platform::EspError> {
        self.into_station(
            seed,
            resources,
            ull_esp_platform::StationNetworkConfig::default(),
        )
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

    pub fn take_i2c0_parts(&mut self) -> Result<I2c0Parts, BoardError> {
        self.i2c0.take().ok_or(BoardError::AlreadyTaken("i2c0"))
    }

    pub fn take_wifi_parts(&mut self) -> Result<WifiParts, BoardError> {
        self.wifi.take().ok_or(BoardError::AlreadyTaken("wifi"))
    }

    pub fn take_status_led_pin(&mut self) -> Result<StatusLedPin, BoardError> {
        self.pins
            .status_led
            .take()
            .ok_or(BoardError::AlreadyTaken("status_led"))
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
