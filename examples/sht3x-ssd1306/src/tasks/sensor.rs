use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Delay, Duration, Timer};
use ull_sht3x::{Address, Repeatability, Sht3x};

use crate::app::{APP_RESOURCES, Reading};

#[embassy_executor::task]
pub async fn sensor_task(
    i2c: I2cDevice<'static, CriticalSectionRawMutex, ull_esp_platform::i2c::SharedI2c>,
) {
    let mut delay = Delay;
    let mut sensor = Sht3x::with_address(i2c, Address::DEFAULT);

    if let Err(err) = sensor.soft_reset_async(&mut delay).await {
        log::warn!("soft reset failed: {err}");
    }

    if let Err(err) = sensor.clear_status_and_wait_async(&mut delay).await {
        log::warn!("clear status failed: {err}");
    }

    loop {
        match sensor
            .measure_raw_async(&mut delay, Repeatability::High)
            .await
        {
            Ok(raw) => {
                let fixed = raw.to_fixed_point();
                let reading = Reading {
                    temperature_millicelsius: fixed.temperature_millicelsius,
                    relative_humidity_hundredths: fixed.relative_humidity_hundredths,
                };
                let is_negative = fixed.temperature_millicelsius < 0;
                let temperature_abs = fixed.temperature_millicelsius.abs();

                APP_RESOURCES.display_reading.signal(reading);
                APP_RESOURCES.enqueue_publish_reading(reading);

                log::info!(
                    "temp = {}{}.{:03} C, rh = {}.{:02} %",
                    if is_negative { "-" } else { "" },
                    temperature_abs / 1000,
                    temperature_abs % 1000,
                    fixed.relative_humidity_hundredths / 100,
                    fixed.relative_humidity_hundredths % 100
                );
            }
            Err(err) => {
                log::warn!("sht3x read failed: {err}");
            }
        }

        Timer::after(Duration::from_secs(1)).await;
    }
}
