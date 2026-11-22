use core::fmt::Write;

use embassy_net::{Stack, tcp::TcpSocket};
use embassy_time::Duration;
use heapless::String as HeaplessString;
use ull_esp_platform::http;

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
    let mut response_buffer = [0u8; 512];

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

    let result = async {
        http::send_request(
            &mut socket,
            (config.server_ip, config.port),
            request.as_bytes(),
        )
        .await?;
        let (head, _) = http::read_response_head(&mut socket, &mut response_buffer).await?;
        Ok::<_, AppError>(head.status)
    }
    .await;
    http::close(&mut socket).await;

    match result? {
        status if (200..300).contains(&status) => Ok(()),
        status => Err(AppError::HttpStatus(status)),
    }
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
