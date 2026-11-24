//! First-board bring-up for ull-pocket: the user LED shows which button is
//! held (UP red, OK blue, DN both), and the log reports the buttons, USB
//! power and the wake lines once a second.

use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};
use ull_esp_board_pocket::{Board, BoardError};

pub async fn run(_spawner: Spawner) -> Result<(), BoardError> {
    let mut board = Board::init();
    let buttons = board.take_buttons()?;
    let wake = board.take_wake_inputs()?;
    let mut led = board.take_user_led()?;
    let _charger = board.take_charger()?; // charging on

    let mut ticker = Ticker::every(Duration::from_millis(50));
    let mut tick = 0u32;
    loop {
        let up = buttons.up.is_low();
        let ok = buttons.ok.is_low();
        let dn = buttons.dn.is_low();
        led.set(up || dn, ok || dn);

        if tick.is_multiple_of(20) {
            log::info!(
                "buttons up={up} ok={ok} dn={dn}; usb={} bat_alert={} acc_int1={} baro_int={}",
                wake.usb_power.is_low(),
                wake.battery_alert.is_low(),
                wake.accel_int1.is_high(),
                wake.baro_int.is_low(),
            );
        }
        tick = tick.wrapping_add(1);
        ticker.next().await;
    }
}
