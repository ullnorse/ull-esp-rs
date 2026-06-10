use core::fmt::Write;

use heapless::String as HeaplessString;

use crate::error::AppError;
use crate::reading::Reading;

pub fn json_body(reading: Reading) -> Result<HeaplessString<96>, AppError> {
    let mut body = HeaplessString::new();

    write!(
        body,
        "{{\"temperature_millicelsius\":{},\"relative_humidity_hundredths\":{}}}",
        reading.temperature_millicelsius, reading.relative_humidity_hundredths,
    )
    .map_err(|_| AppError::BodyBufferTooSmall)?;

    Ok(body)
}
