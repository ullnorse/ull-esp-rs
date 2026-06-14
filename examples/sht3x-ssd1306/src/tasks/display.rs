use core::fmt::Write;

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use heapless::String as HeaplessString;
use ull_ssd1306::{DisplaySize128x64, Rotation, Ssd1306};

use crate::app::APP_RESOURCES;

#[embassy_executor::task]
pub async fn display_task(
    i2c: I2cDevice<'static, CriticalSectionRawMutex, ull_esp_platform::i2c::SharedI2c>,
) {
    let mut display =
        Ssd1306::new(i2c, DisplaySize128x64, Rotation::Rotate180).into_buffered_graphics_mode();
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    loop {
        match display.init_async().await {
            Ok(()) => break,
            Err(err) => {
                log::warn!("display init failed: {err}");
                Timer::after(Duration::from_secs(1)).await;
            }
        }
    }

    display.clear();
    let _ = Text::new("Waiting for sensor...", Point::new(0, 16), style).draw(&mut display);
    if let Err(err) = display.flush_async().await {
        log::warn!("initial display flush failed: {err}");
    }

    loop {
        let reading = APP_RESOURCES.display_reading.wait().await;

        let is_negative = reading.temperature_millicelsius < 0;
        let temp_abs = reading.temperature_millicelsius.abs();
        let temp_whole = temp_abs / 1000;
        let temp_frac = (temp_abs % 1000) / 100;

        let rh_tenths = reading.relative_humidity_hundredths / 10;
        let rh_whole = rh_tenths / 10;
        let rh_frac = rh_tenths % 10;

        let mut line1: HeaplessString<32> = HeaplessString::new();
        let mut line2: HeaplessString<32> = HeaplessString::new();

        if write!(
            line1,
            "Temp: {}{}.{} C",
            if is_negative { "-" } else { "" },
            temp_whole,
            temp_frac
        )
        .is_err()
        {
            log::warn!("display line 1 formatting exceeded buffer");
            continue;
        }

        if write!(line2, "RH:   {}.{} %", rh_whole, rh_frac).is_err() {
            log::warn!("display line 2 formatting exceeded buffer");
            continue;
        }

        display.clear();
        let _ = Text::new("SHT3x", Point::new(0, 12), style).draw(&mut display);
        let _ = Text::new(line1.as_str(), Point::new(0, 30), style).draw(&mut display);
        let _ = Text::new(line2.as_str(), Point::new(0, 46), style).draw(&mut display);

        if let Err(err) = display.flush_async().await {
            log::warn!("display flush failed: {err}");
        }
    }
}
