#![no_std]

mod error;
mod i2c;
mod led;
mod pins;
mod setup;
mod spi;
mod uart;
mod wifi;

use pins::{BoardPins, I2c0Pins, Spi2Pins, Uart2Pins};

use esp_hal::peripherals as p;
use esp_hal::rng::Rng;

pub use error::BoardError;
pub use led::StatusLed;
pub use wifi::{DEFAULT_WIFI_SOCKETS, WifiStation};

pub struct RuntimeParts {
    pub timg0: p::TIMG0<'static>,
    pub cpu_int0: p::FROM_CPU_INTR0<'static>,
}

impl RuntimeParts {
    pub fn start(self) {
        ull_esp_platform::runtime::start(self.timg0, self.cpu_int0);
    }
}

/// The peripherals the DevKit V1 mapping uses. Build it with [`take_board!`],
/// which moves only these fields out of `esp_hal::peripherals::Peripherals`.
pub struct BoardParts {
    pub timg0: p::TIMG0<'static>,
    pub cpu_int0: p::FROM_CPU_INTR0<'static>,
    pub timg1: p::TIMG1<'static>,
    pub flash: p::FLASH<'static>,
    pub wifi: p::WIFI<'static>,
    pub i2c0: p::I2C0<'static>,
    pub spi2: p::SPI2<'static>,
    pub uart2: p::UART2<'static>,
    pub gpio0: p::GPIO0<'static>,
    pub gpio2: p::GPIO2<'static>,
    pub gpio5: p::GPIO5<'static>,
    pub gpio16: p::GPIO16<'static>,
    pub gpio17: p::GPIO17<'static>,
    pub gpio18: p::GPIO18<'static>,
    pub gpio19: p::GPIO19<'static>,
    pub gpio21: p::GPIO21<'static>,
    pub gpio22: p::GPIO22<'static>,
    pub gpio23: p::GPIO23<'static>,
}

/// Moves the board's peripherals out of a `Peripherals` value and builds a
/// [`Board`]. Every other field stays usable afterwards:
///
/// ```ignore
/// let peripherals = esp_hal::init(Board::default_config());
/// let mut board = ull_esp_board_devkit_v1::take_board!(peripherals);
/// let adc = peripherals.ADC1; // not mapped by the board, still yours
/// ```
#[macro_export]
macro_rules! take_board {
    ($peripherals:ident) => {
        $crate::Board::from_parts($crate::BoardParts {
            timg0: $peripherals.TIMG0,
            cpu_int0: $peripherals.FROM_CPU_INTR0,
            timg1: $peripherals.TIMG1,
            flash: $peripherals.FLASH,
            wifi: $peripherals.WIFI,
            i2c0: $peripherals.I2C0,
            spi2: $peripherals.SPI2,
            uart2: $peripherals.UART2,
            gpio0: $peripherals.GPIO0,
            gpio2: $peripherals.GPIO2,
            gpio5: $peripherals.GPIO5,
            gpio16: $peripherals.GPIO16,
            gpio17: $peripherals.GPIO17,
            gpio18: $peripherals.GPIO18,
            gpio19: $peripherals.GPIO19,
            gpio21: $peripherals.GPIO21,
            gpio22: $peripherals.GPIO22,
            gpio23: $peripherals.GPIO23,
        })
    };
}

pub struct Board {
    runtime: Option<RuntimeParts>,
    timg1: Option<p::TIMG1<'static>>,
    flash: Option<p::FLASH<'static>>,
    wifi: Option<wifi::WifiParts>,
    i2c0: Option<i2c::I2c0Parts>,
    spi2: Option<spi::Spi2Parts>,
    uart2: Option<uart::Uart2Parts>,
    pins: BoardPins,
}

impl Board {
    /// Initializes esp-hal at max clock and keeps only the board's peripherals.
    ///
    /// The rest are dropped; use [`take_board!`] when the app needs any of them.
    pub fn init() -> Self {
        let peripherals = esp_hal::init(Self::default_config());
        take_board!(peripherals)
    }

    pub fn default_config() -> esp_hal::Config {
        ull_esp_platform::runtime::max_clock_config()
    }

    pub fn from_parts(parts: BoardParts) -> Self {
        Self {
            runtime: Some(RuntimeParts {
                timg0: parts.timg0,
                cpu_int0: parts.cpu_int0,
            }),
            timg1: Some(parts.timg1),
            flash: Some(parts.flash),
            wifi: Some(wifi::WifiParts {
                peripheral: parts.wifi,
            }),
            i2c0: Some(i2c::I2c0Parts {
                controller: parts.i2c0,
                pins: I2c0Pins {
                    scl: parts.gpio22,
                    sda: parts.gpio21,
                },
            }),
            spi2: Some(spi::Spi2Parts {
                controller: parts.spi2,
                pins: Spi2Pins {
                    sck: parts.gpio18,
                    miso: parts.gpio19,
                    mosi: parts.gpio23,
                },
            }),
            uart2: Some(uart::Uart2Parts {
                controller: parts.uart2,
                pins: Uart2Pins {
                    tx: parts.gpio17,
                    rx: parts.gpio16,
                },
            }),
            pins: BoardPins {
                boot_button: Some(parts.gpio0),
                status_led: Some(parts.gpio2),
                spi2_cs: Some(parts.gpio5),
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

    /// Starts the watchdog on TIMG1: reboots when one of `pulses` is late,
    /// and the hardware watchdog resets the chip when tasks stop running.
    /// See `ull_esp_platform::watchdog`.
    pub fn start_watchdog(
        &mut self,
        spawner: embassy_executor::Spawner,
        pulses: &'static [&'static ull_esp_platform::watchdog::Pulse],
    ) -> Result<(), BoardError> {
        let timg1 = self.timg1.take().ok_or(BoardError::AlreadyTaken("timg1"))?;
        ull_esp_platform::watchdog::start(spawner, timg1, pulses)?;
        Ok(())
    }

    pub fn take_flash_storage(
        &mut self,
    ) -> Result<ull_esp_platform::flash::FlashStorageDevice, BoardError> {
        let flash = self.flash.take().ok_or(BoardError::AlreadyTaken("flash"))?;
        Ok(ull_esp_platform::flash::init_flash_storage(flash))
    }

    fn wifi_seed() -> u64 {
        let rng = Rng::new();
        ((rng.random() as u64) << 32) | rng.random() as u64
    }
}
