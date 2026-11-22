//! A tiny HTTP/1.1 client over an embassy-net TCP socket: send a prepared
//! request, read the response head, then read the body from the socket.
//! Parsing lives in `ull_esp_core::http` and is tested on the host.

use embassy_net::IpEndpoint;
use embassy_net::tcp::TcpSocket;
use embedded_io_async::Write as _;
use thiserror::Error;

pub use ull_esp_core::http::{BaseUrl, HttpError, ResponseHead, parse_response_head};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Error)]
pub enum HttpClientError {
    #[error("tcp connect failed")]
    Connect,
    #[error("tcp write failed")]
    Write,
    #[error("tcp read failed")]
    Read,
    #[error("connection closed before the response head ended")]
    ConnectionClosed,
    #[error("response head does not fit the {0}-byte buffer")]
    HeadTooLarge(usize),
    #[error("malformed HTTP response")]
    Malformed,
    #[error("invalid Content-Length")]
    InvalidContentLength,
    #[error("response body does not fit the {0}-byte buffer")]
    BodyTooLarge(usize),
}

impl From<HttpError> for HttpClientError {
    fn from(value: HttpError) -> Self {
        match value {
            HttpError::Malformed => Self::Malformed,
            HttpError::InvalidContentLength => Self::InvalidContentLength,
        }
    }
}

/// Connects and sends a complete request (head and body).
pub async fn send_request(
    socket: &mut TcpSocket<'_>,
    remote: impl Into<IpEndpoint>,
    request: &[u8],
) -> Result<(), HttpClientError> {
    socket
        .connect(remote)
        .await
        .map_err(|_| HttpClientError::Connect)?;
    socket
        .write_all(request)
        .await
        .map_err(|_| HttpClientError::Write)?;
    socket.flush().await.map_err(|_| HttpClientError::Write)
}

/// Reads until the response head is complete.
///
/// Returns the head and the body bytes that arrived with it; read the rest of
/// the body from the socket.
pub async fn read_response_head<'b>(
    socket: &mut TcpSocket<'_>,
    buffer: &'b mut [u8],
) -> Result<(ResponseHead<'b>, &'b [u8]), HttpClientError> {
    let mut received = 0;

    loop {
        if received == buffer.len() {
            return Err(HttpClientError::HeadTooLarge(buffer.len()));
        }

        let count = socket
            .read(&mut buffer[received..])
            .await
            .map_err(|_| HttpClientError::Read)?;
        if count == 0 {
            return Err(HttpClientError::ConnectionClosed);
        }
        received += count;

        if parse_response_head(&buffer[..received])?.is_some() {
            break;
        }
    }

    let buffer: &'b [u8] = &buffer[..received];
    let head = parse_response_head(buffer)?.ok_or(HttpClientError::Malformed)?;
    Ok((head, &buffer[head.header_end..]))
}

/// Closes the write side and flushes, ignoring errors: used on every exit path.
pub async fn close(socket: &mut TcpSocket<'_>) {
    socket.close();
    let _ = socket.flush().await;
}

/// Reads a small response body into `buffer`: the bytes that came with the
/// head, then up to `Content-Length` (or end of stream) from the socket.
pub async fn read_small_body<'b>(
    socket: &mut TcpSocket<'_>,
    head: &ResponseHead<'_>,
    body_prefix: &[u8],
    buffer: &'b mut [u8],
) -> Result<&'b [u8], HttpClientError> {
    let wanted = match head.content_length()? {
        Some(length) if length > buffer.len() => {
            return Err(HttpClientError::BodyTooLarge(buffer.len()));
        }
        Some(length) => length,
        None => buffer.len(),
    };
    let mut received = body_prefix.len().min(wanted);
    buffer[..received].copy_from_slice(&body_prefix[..received]);
    while received < wanted {
        let count = socket
            .read(&mut buffer[received..wanted])
            .await
            .map_err(|_| HttpClientError::Read)?;
        if count == 0 {
            break;
        }
        received += count;
    }
    Ok(&buffer[..received])
}
