use esp_hal::Async;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;

use super::{Board, BoardError};

/// Inputs need no internal pull: every line has a pull-up on the board
/// (buttons R14/R29/R24, BARO_INT R35) or a push-pull driver.
fn input(pin: impl esp_hal::gpio::InputPin + 'static) -> Input<'static> {
    Input::new(pin, InputConfig::default().with_pull(Pull::None))
}

/// The three side buttons. Each reads low while pressed.
pub struct Buttons {
    /// UP, IO2: wakes from deep sleep.
    pub up: Input<'static>,
    /// OK, IO5: wakes from deep sleep.
    pub ok: Input<'static>,
    /// DN, IO9: not a low-power pin, so it cannot wake the chip; poll it.
    /// Held at power-up it starts the ROM download mode.
    pub dn: Input<'static>,
}

/// The red/blue user LED. Common anode: a pin driven low lights its colour.
pub struct UserLed {
    red: Output<'static>,
    blue: Output<'static>,
}

impl UserLed {
    pub fn set(&mut self, red: bool, blue: bool) {
        self.red
            .set_level(if red { Level::Low } else { Level::High });
        self.blue
            .set_level(if blue { Level::Low } else { Level::High });
    }

    pub fn off(&mut self) {
        self.set(false, false);
    }
}

/// The charger enable (BQ24073 /CE on IO15).
pub struct Charger {
    chg_dis: Output<'static>,
}

impl Charger {
    /// Pauses charging. The battery still supplies the system.
    pub fn pause(&mut self) {
        self.chg_dis.set_high();
    }

    pub fn resume(&mut self) {
        self.chg_dis.set_low();
    }
}

/// Lines that can wake the chip from deep sleep, besides UP and OK. All are
/// low-power GPIOs and read low when active.
pub struct WakeInputs {
    /// MAX17048 alert, IO0. Clear the alert before sleeping.
    pub battery_alert: Input<'static>,
    /// USB power present, IO1 (low = present).
    pub usb_power: Input<'static>,
    /// LIS2DW12 INT1, IO3. Active-low only after CTRL3 H_LACTIVE is set.
    pub accel_int1: Input<'static>,
    /// BMP580 INT, IO4. Open-drain active-low; R35 holds it high.
    pub baro_int: Input<'static>,
}

/// The e-paper panel's bus and control lines (SSD1681). The SPI bus is
/// write-only: the panel has no MISO line here.
pub struct Epd {
    pub spi: Spi<'static, Async>,
    /// Chip select, IO21, active low; R26 holds it high.
    pub cs: Output<'static>,
    /// Data/command, IO20: low = command, high = data.
    pub dc: Output<'static>,
    /// Reset, IO19, active low; R25 holds it high.
    pub rst: Output<'static>,
    /// Busy, IO18: high while the panel refreshes.
    pub busy: Input<'static>,
}

impl Board {
    pub fn take_buttons(&mut self) -> Result<Buttons, BoardError> {
        let pins = &mut self.pins;
        let (Some(up), Some(ok), Some(dn)) =
            (pins.btn_up.take(), pins.btn_ok.take(), pins.btn_dn.take())
        else {
            return Err(BoardError::AlreadyTaken("buttons"));
        };
        Ok(Buttons {
            up: input(up),
            ok: input(ok),
            dn: input(dn),
        })
    }

    /// The user LED, off.
    pub fn take_user_led(&mut self) -> Result<UserLed, BoardError> {
        let (Some(red), Some(blue)) = (self.pins.led_red.take(), self.pins.led_blue.take()) else {
            return Err(BoardError::AlreadyTaken("user led"));
        };
        Ok(UserLed {
            red: Output::new(red, Level::High, OutputConfig::default()),
            blue: Output::new(blue, Level::High, OutputConfig::default()),
        })
    }

    /// The charger control, charging on.
    pub fn take_charger(&mut self) -> Result<Charger, BoardError> {
        let pin = self
            .pins
            .chg_dis
            .take()
            .ok_or(BoardError::AlreadyTaken("charger"))?;
        Ok(Charger {
            chg_dis: Output::new(pin, Level::Low, OutputConfig::default()),
        })
    }

    pub fn take_wake_inputs(&mut self) -> Result<WakeInputs, BoardError> {
        let pins = &mut self.pins;
        let (Some(bat), Some(usb), Some(acc), Some(baro)) = (
            pins.bat_alert.take(),
            pins.usb_pgood.take(),
            pins.acc_int1.take(),
            pins.baro_int.take(),
        ) else {
            return Err(BoardError::AlreadyTaken("wake inputs"));
        };
        Ok(WakeInputs {
            battery_alert: input(bat),
            usb_power: input(usb),
            accel_int1: input(acc),
            baro_int: input(baro),
        })
    }

    /// The e-paper bus: SPI mode 0, data sampled on the rising edge
    /// (GDEY0154D67 datasheet 7-3-1-2, p.22-23). The panel datasheet gives
    /// no clock limit, so this runs at a conservative 4 MHz: a full 200x200
    /// frame (5000 bytes) takes about 10 ms, nothing next to a refresh.
    pub fn take_epd(&mut self) -> Result<Epd, BoardError> {
        let pins = self
            .pins
            .epd
            .take()
            .ok_or(BoardError::AlreadyTaken("epd"))?;
        let config = Config::default()
            .with_frequency(Rate::from_mhz(4))
            .with_mode(Mode::_0);
        let spi = Spi::new(pins.spi, config)?
            .with_sck(pins.sck)
            .with_mosi(pins.mosi)
            .into_async();
        Ok(Epd {
            spi,
            cs: Output::new(pins.cs, Level::High, OutputConfig::default()),
            dc: Output::new(pins.dc, Level::High, OutputConfig::default()),
            rst: Output::new(pins.rst, Level::High, OutputConfig::default()),
            busy: input(pins.busy),
        })
    }
}
