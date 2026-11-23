//! Minimal HTTP/1.1 response-head parsing for small device clients.

use core::net::Ipv4Addr;
use core::str;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HttpError {
    /// The status line or a header line is malformed, or the head is not UTF-8.
    Malformed,
    /// `Content-Length` is present but not a decimal number.
    InvalidContentLength,
}

/// A parsed response head. Header lookups borrow from the receive buffer.
#[derive(Copy, Clone, Debug)]
pub struct ResponseHead<'a> {
    pub status: u16,
    /// Offset of the first body byte in the buffer that was parsed.
    pub header_end: usize,
    headers: &'a str,
}

impl<'a> ResponseHead<'a> {
    /// Returns the trimmed value of the first header named `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&'a str> {
        find_header(self.headers, name)
    }

    pub fn content_length(&self) -> Result<Option<usize>, HttpError> {
        self.header("content-length")
            .map(|value| value.parse().map_err(|_| HttpError::InvalidContentLength))
            .transpose()
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Parses the head at the start of `response`.
///
/// Returns `Ok(None)` while the blank line ending the head has not arrived yet,
/// so callers can keep reading into the same buffer.
pub fn parse_response_head(response: &[u8]) -> Result<Option<ResponseHead<'_>>, HttpError> {
    let Some(header_end) = find_header_end(response) else {
        return Ok(None);
    };

    let head = str::from_utf8(&response[..header_end]).map_err(|_| HttpError::Malformed)?;
    let (status_line, headers) = head.split_once("\r\n").ok_or(HttpError::Malformed)?;
    let status = parse_status_line(status_line)?;

    let all_lines_valid = headers
        .split("\r\n")
        .filter(|line| !line.is_empty())
        .all(|line| line.contains(':'));
    if !all_lines_valid {
        return Err(HttpError::Malformed);
    }

    Ok(Some(ResponseHead {
        status,
        header_end,
        headers,
    }))
}

fn find_header_end(response: &[u8]) -> Option<usize> {
    response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn parse_status_line(line: &str) -> Result<u16, HttpError> {
    let mut parts = line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/") {
        return Err(HttpError::Malformed);
    }

    let code = parts.next().ok_or(HttpError::Malformed)?;
    if code.len() != 3 {
        return Err(HttpError::Malformed);
    }
    code.parse().map_err(|_| HttpError::Malformed)
}

/// A parsed request head, for the device's own small servers.
#[derive(Copy, Clone, Debug)]
pub struct RequestHead<'a> {
    pub method: &'a str,
    /// The request target without the query string.
    pub path: &'a str,
    /// Offset of the first body byte in the buffer that was parsed.
    pub header_end: usize,
    headers: &'a str,
}

impl<'a> RequestHead<'a> {
    pub fn header(&self, name: &str) -> Option<&'a str> {
        find_header(self.headers, name)
    }

    pub fn content_length(&self) -> Result<usize, HttpError> {
        self.header("content-length").map_or(Ok(0), |value| {
            value.parse().map_err(|_| HttpError::InvalidContentLength)
        })
    }
}

/// Parses the request head at the start of `request`; `Ok(None)` until the
/// blank line ending the head has arrived.
pub fn parse_request_head(request: &[u8]) -> Result<Option<RequestHead<'_>>, HttpError> {
    let Some(header_end) = find_header_end(request) else {
        return Ok(None);
    };
    let head = str::from_utf8(&request[..header_end]).map_err(|_| HttpError::Malformed)?;
    let (request_line, headers) = head.split_once("\r\n").ok_or(HttpError::Malformed)?;
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HttpError::Malformed);
    };
    if method.is_empty() || !version.starts_with("HTTP/1.") {
        return Err(HttpError::Malformed);
    }
    let path = target.split_once('?').map_or(target, |(path, _)| path);
    Ok(Some(RequestHead {
        method,
        path,
        header_end,
        headers,
    }))
}

fn find_header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.split("\r\n").find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then_some(value.trim())
    })
}

/// A plain-HTTP base URL with a literal IPv4 host, like `http://192.168.1.5:3000`.
/// Devices here have no DNS client, so host names are rejected.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct BaseUrl<'a> {
    /// `host[:port]`, for the `Host` header.
    pub authority: &'a str,
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl<'a> BaseUrl<'a> {
    pub fn parse(url: &'a str) -> Option<Self> {
        let authority = url.strip_prefix("http://")?;
        let authority = authority.strip_suffix('/').unwrap_or(authority);
        if authority.is_empty() || authority.contains('/') {
            return None;
        }

        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().ok()?),
            None => (authority, 80),
        };

        Some(Self {
            authority,
            ip: host.parse().ok()?,
            port,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_base_urls() {
        assert_eq!(
            BaseUrl::parse("http://192.168.100.11:3000/"),
            Some(BaseUrl {
                authority: "192.168.100.11:3000",
                ip: Ipv4Addr::new(192, 168, 100, 11),
                port: 3000,
            })
        );
        assert_eq!(
            BaseUrl::parse("http://10.0.0.1").map(|url| url.port),
            Some(80)
        );
        for bad in [
            "https://10.0.0.1",
            "http://",
            "http://10.0.0.1/api",
            "http://fleet.local:3000",
            "http://10.0.0.1:99999",
            "10.0.0.1:3000",
        ] {
            assert_eq!(BaseUrl::parse(bad), None, "{bad}");
        }
    }

    const RESPONSE: &[u8] =
        b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nX-Thing:  value \r\n\r\nbody";

    #[test]
    fn parses_request_heads() {
        let request =
            b"POST /save?x=1 HTTP/1.1\r\nHost: 192.168.4.1\r\nContent-Length: 5\r\n\r\nssid=";
        let head = parse_request_head(request).unwrap().unwrap();
        assert_eq!((head.method, head.path), ("POST", "/save"));
        assert_eq!(head.content_length(), Ok(5));
        assert_eq!(&request[head.header_end..], b"ssid=");
        assert!(parse_request_head(b"GET / HTTP/1.1\r\n").unwrap().is_none());
        assert_eq!(
            parse_request_head(b"GET / HTTP/1.1\r\n\r\n")
                .unwrap()
                .unwrap()
                .content_length(),
            Ok(0)
        );
        assert!(parse_request_head(b"GET /\r\n\r\n").is_err());
    }

    #[test]
    fn incomplete_head_returns_none() {
        assert_eq!(
            parse_response_head(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n")
                .unwrap()
                .map(|head| head.status),
            None
        );
    }

    #[test]
    fn parses_status_headers_and_body_offset() {
        let head = parse_response_head(RESPONSE).unwrap().unwrap();
        assert_eq!(head.status, 200);
        assert!(head.is_success());
        assert_eq!(&RESPONSE[head.header_end..], b"body");
        assert_eq!(head.content_length(), Ok(Some(12)));
        assert_eq!(head.header("x-thing"), Some("value"));
        assert_eq!(head.header("missing"), None);
    }

    #[test]
    fn status_without_reason_phrase_parses() {
        let head = parse_response_head(b"HTTP/1.1 204\r\n\r\n")
            .unwrap()
            .unwrap();
        assert_eq!(head.status, 204);
        assert_eq!(head.content_length(), Ok(None));
    }

    #[test]
    fn rejects_malformed_heads() {
        for response in [
            &b"garbage\r\n\r\n"[..],
            b"HTTP/1.1 2000 OK\r\n\r\n",
            b"HTTP/1.1 abc OK\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nno colon here\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nBad: \xff\r\n\r\n",
        ] {
            assert_eq!(
                parse_response_head(response).map(|head| head.map(|head| head.status)),
                Err(HttpError::Malformed),
                "{response:?}"
            );
        }
    }

    #[test]
    fn rejects_non_numeric_content_length() {
        let head = parse_response_head(b"HTTP/1.1 200 OK\r\nContent-Length: ten\r\n\r\n")
            .unwrap()
            .unwrap();
        assert_eq!(head.content_length(), Err(HttpError::InvalidContentLength));
    }
}
