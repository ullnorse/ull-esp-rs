use core::str;

use embassy_net::tcp::TcpSocket;

use crate::error::AppError;

pub struct HttpResponseHead {
    pub status: u16,
    pub content_length: Option<usize>,
    pub application_image_sha256: Option<[u8; 32]>,
    pub header_end: usize,
}

pub async fn read_response_head(
    socket: &mut TcpSocket<'_>,
    response_buffer: &mut [u8],
) -> Result<(HttpResponseHead, usize), AppError> {
    let mut received = 0usize;

    loop {
        if received == response_buffer.len() {
            return Err(AppError::ResponseBufferTooSmall);
        }

        let count = socket
            .read(&mut response_buffer[received..])
            .await
            .map_err(|_| AppError::TcpReadFailed)?;

        if count == 0 {
            return Err(AppError::InvalidHttpResponse);
        }

        received += count;

        if let Some(header_end) = find_http_header_end(&response_buffer[..received]) {
            let head = parse_http_response_head(&response_buffer[..received], header_end)?;
            return Ok((head, received));
        }
    }
}

pub async fn close_socket(socket: &mut TcpSocket<'_>) {
    socket.close();
    let _ = socket.flush().await;
}

fn find_http_header_end(response: &[u8]) -> Option<usize> {
    response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn parse_http_response_head(
    response: &[u8],
    header_end: usize,
) -> Result<HttpResponseHead, AppError> {
    let head =
        str::from_utf8(&response[..header_end]).map_err(|_| AppError::InvalidHttpResponse)?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or(AppError::InvalidHttpResponse)?;
    let status = parse_http_status_line(status_line)?;
    let mut content_length = None;
    let mut application_image_sha256 = None;

    for line in lines {
        if line.is_empty() {
            continue;
        }

        let Some((name, value)) = line.split_once(':') else {
            return Err(AppError::InvalidHttpResponse);
        };

        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(value.parse().map_err(|_| AppError::InvalidContentLength)?);
        } else if name.eq_ignore_ascii_case("x-application-image-sha256") {
            application_image_sha256 =
                Some(parse_sha256_hex(value).ok_or(AppError::InvalidApplicationImageSha256)?);
        }
    }

    Ok(HttpResponseHead {
        status,
        content_length,
        application_image_sha256,
        header_end,
    })
}

fn parse_http_status_line(status_line: &str) -> Result<u16, AppError> {
    status_line
        .split(' ')
        .nth(1)
        .and_then(|digits| digits.parse::<u16>().ok())
        .ok_or(AppError::InvalidHttpResponse)
}

fn parse_sha256_hex(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }

    let mut digest = [0u8; 32];
    let bytes = value.as_bytes();

    let mut index = 0;
    while index < digest.len() {
        let high = decode_hex(bytes[index * 2])?;
        let low = decode_hex(bytes[index * 2 + 1])?;
        digest[index] = (high << 4) | low;
        index += 1;
    }

    Some(digest)
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
