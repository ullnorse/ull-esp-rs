#![no_std]
//! Board support for ull-pocket rev 0.10 (ESP32-C6-MINI-1-N4). The pin map
//! comes from the schematic netlist; the rules come from PARTS.md in the
//! ull-pocket repo.
//!
//! | GPIO | Net | Use |
//! |---|---|---|
//! | IO0 | BAT_ALRT_N | MAX17048 alert, low while set (LP, wake) |
//! | IO1 | USB_PGOOD_N | low while USB power is present (LP, wake) |
//! | IO2 | BTN_N | UP button, low when pressed (LP, wake) |
//! | IO3 | ACC_INT1 | LIS2DW12 INT1 (LP, wake; set active-low in CTRL3) |
//! | IO4 | BARO_INT | BMP580 INT, open-drain active-low, R35 pull-up (LP, wake) |
//! | IO5 | BTN2_N | OK button, low when pressed (LP, wake) |
//! | IO6, IO7 | I2C_SDA, I2C_SCL | SHT40 0x44, BMP580 0x46, LIS2DW12 0x19, MAX17048 0x36 |
//! | IO8 | LED_R | user LED red, low = on (boot strap, R16 pull-up) |
//! | IO9 | BTN3_N | DN button, low when pressed; also BOOT (not LP: poll it) |
//! | IO12, IO13 | USB D-, D+ | native USB Serial/JTAG |
//! | IO14 | LED_B | user LED blue, low = on |
//! | IO15 | CHG_DIS | high pauses charging (BQ24073 /CE); R36 pulls it low |
//! | IO18-IO23 | EPD_BUSY, EPD_RST_N, EPD_DC, EPD_CS_N, EPD_SCK, EPD_MOSI | SSD1681 e-paper |
//!
//! Rules every app must keep (PARTS.md, "Firmware rules"):
//!
//! - I2C runs at 100 kHz: the 10k pull-ups are too slow for 400 kHz. Never
//!   leave SDA or SCL driven low in sleep (330 uA per line).
//! - Before deep sleep: clear the MAX17048 alert (BAT_ALRT_N would wake the
//!   chip at once), enable the BMP580 INT before arming the wake mask, hold
//!   EPD DC/SCK/MOSI, and hold CHG_DIS low or leave it an input.
//! - Stay awake while USB_PGOOD_N is low: USB is off in deep sleep, so a
//!   sleeping board cannot be flashed.
//! - No Wi-Fi below about 3.55 V (MAX17048 reading): the LDO drops out during
//!   transmit bursts.
//!
//! Not checked on hardware yet: the board has not been built.

mod error;
mod i2c;
mod io;
mod pins;

use esp_hal::peripherals as p;

pub use error::BoardError;
pub use io::{Buttons, Charger, Epd, UserLed, WakeInputs};

/// The peripherals the ull-pocket mapping uses. Build it with [`take_board!`],
/// which moves only these fields out of `esp_hal::peripherals::Peripherals`.
pub struct BoardParts {
    pub timg0: p::TIMG0<'static>,
    pub cpu_int0: p::FROM_CPU_INTR0<'static>,
    pub timg1: p::TIMG1<'static>,
    pub flash: p::FLASH<'static>,
    pub wifi: p::WIFI<'static>,
    pub i2c0: p::I2C0<'static>,
    pub spi2: p::SPI2<'static>,
    pub gpio0: p::GPIO0<'static>,
    pub gpio1: p::GPIO1<'static>,
    pub gpio2: p::GPIO2<'static>,
    pub gpio3: p::GPIO3<'static>,
    pub gpio4: p::GPIO4<'static>,
    pub gpio5: p::GPIO5<'static>,
    pub gpio6: p::GPIO6<'static>,
    pub gpio7: p::GPIO7<'static>,
    pub gpio8: p::GPIO8<'static>,
    pub gpio9: p::GPIO9<'static>,
    pub gpio14: p::GPIO14<'static>,
    pub gpio15: p::GPIO15<'static>,
    pub gpio18: p::GPIO18<'static>,
    pub gpio19: p::GPIO19<'static>,
    pub gpio20: p::GPIO20<'static>,
    pub gpio21: p::GPIO21<'static>,
    pub gpio22: p::GPIO22<'static>,
    pub gpio23: p::GPIO23<'static>,
}

/// Moves the board's peripherals out of a `Peripherals` value and builds a
/// [`Board`]. Every other field stays usable afterwards.
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
            gpio0: $peripherals.GPIO0,
            gpio1: $peripherals.GPIO1,
            gpio2: $peripherals.GPIO2,
            gpio3: $peripherals.GPIO3,
            gpio4: $peripherals.GPIO4,
            gpio5: $peripherals.GPIO5,
            gpio6: $peripherals.GPIO6,
            gpio7: $peripherals.GPIO7,
            gpio8: $peripherals.GPIO8,
            gpio9: $peripherals.GPIO9,
            gpio14: $peripherals.GPIO14,
            gpio15: $peripherals.GPIO15,
            gpio18: $peripherals.GPIO18,
            gpio19: $peripherals.GPIO19,
            gpio20: $peripherals.GPIO20,
            gpio21: $peripherals.GPIO21,
            gpio22: $peripherals.GPIO22,
            gpio23: $peripherals.GPIO23,
        })
    };
}

pub struct Board {
    runtime: Option<(p::TIMG0<'static>, p::FROM_CPU_INTR0<'static>)>,
    timg1: Option<p::TIMG1<'static>>,
    flash: Option<p::FLASH<'static>>,
    wifi: Option<p::WIFI<'static>>,
    i2c0: Option<i2c::I2c0Parts>,
    pins: pins::BoardPins,
}

impl Board {
    /// Initializes esp-hal at max clock, sets up a 64 KiB heap and starts
    /// the async runtime. Keeps only the board's peripherals; use
    /// [`take_board!`] and [`Board::from_parts`] when the app needs others,
    /// then start the runtime with [`Board::start_runtime`].
    pub fn init() -> Self {
        let peripherals = esp_hal::init(ull_esp_platform::runtime::max_clock_config());
        ull_esp_platform::runtime::init_default_heap();
        let mut board = take_board!(peripherals);
        let _ = board.start_runtime();
        board
    }

    pub fn from_parts(parts: BoardParts) -> Self {
        Self {
            runtime: Some((parts.timg0, parts.cpu_int0)),
            timg1: Some(parts.timg1),
            flash: Some(parts.flash),
            wifi: Some(parts.wifi),
            i2c0: Some(i2c::I2c0Parts {
                controller: parts.i2c0,
                sda: parts.gpio6,
                scl: parts.gpio7,
            }),
            pins: pins::BoardPins {
                bat_alert: Some(parts.gpio0),
                usb_pgood: Some(parts.gpio1),
                btn_up: Some(parts.gpio2),
                acc_int1: Some(parts.gpio3),
                baro_int: Some(parts.gpio4),
                btn_ok: Some(parts.gpio5),
                led_red: Some(parts.gpio8),
                btn_dn: Some(parts.gpio9),
                led_blue: Some(parts.gpio14),
                chg_dis: Some(parts.gpio15),
                epd: Some(pins::EpdPins {
                    spi: parts.spi2,
                    busy: parts.gpio18,
                    rst: parts.gpio19,
                    dc: parts.gpio20,
                    cs: parts.gpio21,
                    sck: parts.gpio22,
                    mosi: parts.gpio23,
                }),
            },
        }
    }

    pub fn start_runtime(&mut self) -> Result<(), BoardError> {
        let (timg0, int0) = self
            .runtime
            .take()
            .ok_or(BoardError::AlreadyTaken("runtime"))?;
        ull_esp_platform::runtime::start(timg0, int0);
        Ok(())
    }

    /// Starts the watchdog on TIMG1; see `ull_esp_platform::watchdog`.
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

    /// The Wi-Fi peripheral, for `ull_esp_platform::wifi::start_station`.
    /// Check the battery first: no Wi-Fi below about 3.55 V.
    pub fn take_wifi(&mut self) -> Result<p::WIFI<'static>, BoardError> {
        self.wifi.take().ok_or(BoardError::AlreadyTaken("wifi"))
    }
}
