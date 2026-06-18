mod error;
mod i2c;
mod led;
mod spi;
mod uart;
mod wifi;

use crate::pins::{BoardPins, I2c0Pins, Spi2Pins, Uart2Pins};

use embassy_time::{Duration, Timer};
use esp_hal::rng::Rng;

pub use error::BoardError;
pub use led::StatusLed;
pub use wifi::WifiStation;

pub struct RuntimeParts {
    pub timg0: esp_hal::peripherals::TIMG0<'static>,
    pub sw_interrupt: esp_hal::peripherals::SW_INTERRUPT<'static>,
}

pub struct Board {
    runtime: Option<RuntimeParts>,
    flash: Option<esp_hal::peripherals::FLASH<'static>>,
    wifi: Option<wifi::WifiParts>,
    i2c0: Option<i2c::I2c0Parts>,
    spi2: Option<spi::Spi2Parts>,
    uart2: Option<uart::Uart2Parts>,
    pins: BoardPins,
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
            SPI2: spi2,
            UART2: uart2,
            GPIO22: gpio22,
            GPIO21: gpio21,
            GPIO18: gpio18,
            GPIO19: gpio19,
            GPIO23: gpio23,
            GPIO5: gpio5,
            GPIO17: gpio17,
            GPIO16: gpio16,
            GPIO2: gpio2,
            FLASH: flash,
            WIFI: wifi,
            ..
        } = peripherals;

        Self {
            runtime: Some(RuntimeParts {
                timg0,
                sw_interrupt,
            }),
            flash: Some(flash),
            wifi: Some(wifi::WifiParts { peripheral: wifi }),
            i2c0: Some(i2c::I2c0Parts {
                controller: i2c0,
                pins: I2c0Pins {
                    scl: gpio22,
                    sda: gpio21,
                },
            }),
            spi2: Some(spi::Spi2Parts {
                controller: spi2,
                pins: Spi2Pins {
                    sck: gpio18,
                    miso: gpio19,
                    mosi: gpio23,
                },
            }),
            uart2: Some(uart::Uart2Parts {
                controller: uart2,
                pins: Uart2Pins {
                    tx: gpio17,
                    rx: gpio16,
                },
            }),
            pins: BoardPins {
                status_led: Some(gpio2),
                spi2_cs: Some(gpio5),
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

    pub fn take_flash_storage(
        &mut self,
    ) -> Result<ull_esp_platform::ota::FlashStorageDevice, BoardError> {
        let flash = self.flash.take().ok_or(BoardError::AlreadyTaken("flash"))?;
        Ok(ull_esp_platform::ota::init_flash_storage(flash))
    }

    pub async fn sleep(&self, duration: Duration) {
        Timer::after(duration).await;
    }

    pub async fn sleep_ms(&self, millis: u64) {
        self.sleep(Duration::from_millis(millis)).await;
    }

    fn wifi_seed() -> u64 {
        let rng = Rng::new();
        ((rng.random() as u64) << 32) | rng.random() as u64
    }
}
