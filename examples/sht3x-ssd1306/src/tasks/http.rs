use core::fmt::Write;

use embassy_net::{Stack, tcp::TcpSocket};
use embassy_time::Duration;
use embedded_io_async::Write as _;
use heapless::String as HeaplessString;

use crate::app::APP_RESOURCES;
use crate::app::{Reading, ReadingsConfig};
use crate::error::AppError;

#[embassy_executor::task]
pub async fn http_task(stack: Stack<'static>, config: ReadingsConfig) {
    stack.wait_config_up().await;
    log::info!(
        "http publisher ready for http://{}:{}{}",
        config.server_addr,
        config.port,
        config.path
    );

    loop {
        let reading = APP_RESOURCES.publish_readings.receive().await;

        if !stack.is_config_up() {
            log::warn!("network config down, waiting for DHCP");
            stack.wait_config_up().await;
        }

        if let Err(err) = post_reading(stack, config, reading).await {
            log::warn!("http send failed: {err}");
        }
    }
}

async fn post_reading(
    stack: Stack<'static>,
    config: ReadingsConfig,
    reading: Reading,
) -> Result<(), AppError> {
    let body = json_body(reading)?;
    let mut request: HeaplessString<384> = HeaplessString::new();
    let mut rx_buffer = [0u8; 1024];
    let mut tx_buffer = [0u8; 1024];
    let mut response_buffer = [0u8; 256];

    write!(
        request,
        "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        config.path,
        config.server_addr,
        config.port,
        body.len(),
        body.as_str(),
    )
    .map_err(|_| AppError::RequestBufferTooSmall)?;

    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(5)));
    socket
        .connect((config.server_ip, config.port))
        .await
        .map_err(|_| AppError::TcpConnectFailed)?;
    socket
        .write_all(request.as_bytes())
        .await
        .map_err(|_| AppError::TcpWriteFailed)?;
    socket.flush().await.map_err(|_| AppError::TcpFlushFailed)?;

    let mut received = 0;
    let status = loop {
        if received == response_buffer.len() {
            return Err(AppError::InvalidHttpResponse);
        }

        let count = socket
            .read(&mut response_buffer[received..])
            .await
            .map_err(|_| AppError::InvalidHttpResponse)?;

        if count == 0 {
            return Err(AppError::InvalidHttpResponse);
        }

        received += count;

        if response_buffer[..received].contains(&b'\n') {
            break parse_http_status(&response_buffer[..received])?;
        }
    };

    socket.close();
    let _ = socket.flush().await;

    if !(200..300).contains(&status) {
        return Err(AppError::HttpStatus(status));
    }

    Ok(())
}

fn json_body(reading: Reading) -> Result<HeaplessString<96>, AppError> {
    let mut body = HeaplessString::new();

    write!(
        body,
        "{{\"temperature_millicelsius\":{},\"relative_humidity_hundredths\":{}}}",
        reading.temperature_millicelsius, reading.relative_humidity_hundredths,
    )
    .map_err(|_| AppError::BodyBufferTooSmall)?;

    Ok(body)
}

fn parse_http_status(response: &[u8]) -> Result<u16, AppError> {
    let status_line = response
        .split(|byte| *byte == b'\n')
        .next()
        .ok_or(AppError::InvalidHttpResponse)?;
    let status_line = status_line.strip_suffix(b"\r").unwrap_or(status_line);

    status_line
        .split(|byte| *byte == b' ')
        .nth(1)
        .and_then(|digits| core::str::from_utf8(digits).ok())
        .and_then(|digits| digits.parse::<u16>().ok())
        .ok_or(AppError::InvalidHttpResponse)
}
