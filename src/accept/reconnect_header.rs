use std::convert::TryInto;

use base64::Engine;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{error::{bad_data, ConnResult}, server::connection::read_until_exact};

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct ReconnectToken {
    pub data: [u8; 16]
}

impl ReconnectToken {
    pub fn to_b64(&self) -> Vec<u8> {
        base64::prelude::BASE64_STANDARD.encode(self.data).into_bytes()
    }

    pub fn from_b64(v: &[u8]) -> Option<Self> {
        let data = base64::prelude::BASE64_STANDARD.decode(v).ok()?;
        let data: [u8 ; 16] = data.try_into().ok()?;
        Some(Self { data })
    }
}

#[derive(Debug)]
pub struct ReconnectRequest {
    pub read_offset: Option<u32>,
    pub client_writer_token: Option<ReconnectToken>,
    pub request_writer_token: bool,
}

impl ReconnectRequest {
    pub fn new() -> Self {
        Self {
            read_offset: None,
            client_writer_token: None,
            request_writer_token: false,
        }
    }
}

pub struct ReconnectResponse {
    pub request_writer_token: Option<ReconnectToken>,
}

pub struct ReconnectHeader {
    pub read_offset: Option<u32>,
    pub client_writer_token: Option<ReconnectToken>,
    pub request_writer_token: Option<ReconnectToken>,
}

impl ReconnectHeader {
    fn new(req: &ReconnectRequest, resp: &ReconnectResponse) -> Self {
        // Would be a programmer error!
        assert!(!(req.request_writer_token && resp.request_writer_token.is_none()));
        Self {
            read_offset: req.read_offset,
            client_writer_token: req.client_writer_token,
            request_writer_token: resp.request_writer_token,
        }
    }
}

pub fn reconnect_read_offset(r: &mut ReconnectRequest, d: &Vec<&[u8]>) -> ConnResult<()> {
        if r.read_offset.is_some() {
            return Err(bad_data("\"read_offset\" header was already provided"));
        }
        if d.len() < 2 {
            return Err(bad_data("Too few arguments to \"read_offset\" header"));
        }
        let nums = str::from_utf8(d[1]).map_err(|_| bad_data("\"read_offset\" value is not Unicode"))?;
        let len = str::parse::<u32>(nums).map_err(|_| bad_data("\"read_offset\" value is an invalid number"))?;
        r.read_offset = Some(len);
        Ok(())
}

pub fn reconnect_request_writer_token(r: &mut ReconnectRequest, _d: &Vec<&[u8]>) -> ConnResult<()> {
        if r.request_writer_token {
            return Err(bad_data("\"request_writer_token\" header was already provided"));
        }
        r.request_writer_token = true;
        Ok(())
}

pub fn reconnect_client_writer_token(r: &mut ReconnectRequest, d: &Vec<&[u8]>) -> ConnResult<()> {
        if r.client_writer_token.is_some() {
            return Err(bad_data("\"client_writer_token\" header was already provided"));
        }
        if d.len() < 2 {
            return Err(bad_data("Too few arguments to \"client_writer_token\" header"));
        }
        let token = ReconnectToken::from_b64(d[1]).ok_or_else(|| bad_data("\"read_offset\" value is not Unicode"))?;
        r.client_writer_token = Some(token);
        Ok(())
}

pub fn read_one_header(r: &mut ReconnectRequest, d: &[u8]) -> ConnResult<()> {
        let pieces: Vec<&[u8]> = d.split(|c| c == &b'/').collect();
        assert!(pieces.len() >= 1);
        let cmd = str::from_utf8(pieces[0]).map_err(|_| bad_data("Reconnect header name field is not Unicode"))?;
        match cmd {
            "read_offset" => reconnect_read_offset(r, &pieces)?,
            "request_writer_token" => reconnect_request_writer_token(r, &pieces)?,
            "client_writer_token" => reconnect_client_writer_token(r, &pieces)?,
            _ => return Err(bad_data(format!("Unknown reconnect header type: {}", cmd)))
        }
        Ok(())
}

pub async fn read_reconnect_headers(w: &mut (dyn AsyncBufRead + Unpin)) -> ConnResult<ReconnectRequest> {
    let mut req = ReconnectRequest::new();

    // Headers are zero byte delimited, just like original connection header.
    loop {
        let buf = w.fill_buf().await?;
        if buf.len() == 0 {
            // EOF.
            return Ok(req);
        }
        let data_start = buf[0];
        if data_start < b'a' || data_start > b'z' {
            // All reconnect headers are lowercase. Stop processing.
            // Important: this ensures we don't parse original connection header, which starts with
            // 'P' or 'G'.
            return Ok(req);
        }
        let mut line = Vec::<u8>::new();
        read_until_exact(&mut w.take(1024), b'\0', &mut line)
            .await
            .map_err(|_| bad_data("Reconnect header is incomplete"))?;
        line.pop(); // remove trailing '\0'
        read_one_header(&mut req, &line)?;
    }
}

pub async fn write_request_writer_token(w: &mut (dyn AsyncWrite + Unpin), token: ReconnectToken) -> ConnResult<()> {
    w.write(b"request_writer_token/").await?;
    let token = token.to_b64();
    w.write(&token).await?;
    w.write(b"\0").await?;
    Ok(())
}

// This is used later, in contest of replay writer where we know what the offset should be.
pub async fn write_client_writer_token(w: &mut (dyn AsyncWrite + Unpin), offset: u32) -> ConnResult<()> {
    w.write(b"client_writer_token/").await?;
    let s = offset.to_string();
    w.write(&s.as_bytes()).await?;
    w.write(b"\0").await?;
    Ok(())
}

pub async fn write_reconnect_response(w: &mut (dyn AsyncWrite + Unpin), req: &ReconnectRequest, resp: &ReconnectResponse) -> ConnResult<ReconnectHeader> {
    let rh = ReconnectHeader::new(req, resp);
    if let Some(t) = rh.request_writer_token {
        write_request_writer_token(w, t).await?;
    }
    Ok(rh)
}

#[cfg(test)]
mod test {
    use crate::error::ConnectionError;

    use super::*;
    use tokio::io::{BufStream, DuplexStream};

    const BAD_UNICODE: &'static [u8] = b"\xc3\x28";

    fn bufs() -> (BufStream<DuplexStream>, BufStream<DuplexStream>) {
        let (my, their) = tokio::io::duplex(1024 * 1024);
        let my = BufStream::new(my);
        let their = BufStream::new(their);
        (my, their)
    }

    #[tokio::test]
    async fn test_reconnect_read_empty() {
        let (mut my, mut their) = bufs();

        my.shutdown().await.unwrap();
        let request = read_reconnect_headers(&mut their).await.unwrap();

        assert!(request.read_offset == None);
        assert!(request.request_writer_token == false);
        assert!(request.client_writer_token == None);
    }

    #[tokio::test]
    async fn test_reconnect_read_skips_connection_header() {
        let (mut my, mut their) = bufs();
        my.write_all(b"G/12423353/name\0").await.unwrap();
        my.shutdown().await.unwrap();
        let request = read_reconnect_headers(&mut their).await.unwrap();
        assert!(request.read_offset == None);
        assert!(request.request_writer_token == false);
        assert!(request.client_writer_token == None);
    }

    #[tokio::test]
    async fn test_reconnect_read_error_too_much_data() {
        let (mut my, mut their) = bufs();

        for _ in [0.10000] {
            my.write_all(b"foobar").await.unwrap();
        }
        my.write_all(b"\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_error_non_unicode_name() {
        let (mut my, mut their) = bufs();

        // Ensure we're not mistaken for connection header
        my.write_all(b"foo").await.unwrap();
        my.write_all(BAD_UNICODE).await.unwrap();
        my.write_all(b"/foobar\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_error_unknown_name() {
        let (mut my, mut their) = bufs();

        my.write_all(b"foobar/baz\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_read_offset() {
        let (mut my, mut their) = bufs();

        my.write_all(b"read_offset/100\0").await.unwrap();
        my.shutdown().await.unwrap();
        let request = read_reconnect_headers(&mut their).await.unwrap();

        assert!(request.read_offset == Some(100));
        assert!(request.request_writer_token == false);
        assert!(request.client_writer_token == None);
    }

    #[tokio::test]
    async fn test_reconnect_read_read_offset_error_missing_arg() {
        let (mut my, mut their) = bufs();
        my.write_all(b"read_offset\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_read_offset_error_not_unicode() {
        let (mut my, mut their) = bufs();
        my.write_all(b"read_offset/").await.unwrap();
        my.write_all(BAD_UNICODE).await.unwrap();
        my.write_all(b"\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_read_offset_error_invalid_number() {
        let (mut my, mut their) = bufs();
        my.write_all(b"read_offset/deadbeef\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_read_offset_error_negative_number() {
        let (mut my, mut their) = bufs();
        my.write_all(b"read_offset/-10\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_request_writer_token() {
        let (mut my, mut their) = bufs();

        my.write_all(b"request_writer_token\0").await.unwrap();
        my.shutdown().await.unwrap();
        let request = read_reconnect_headers(&mut their).await.unwrap();

        assert!(request.read_offset == None);
        assert!(request.request_writer_token == true);
        assert!(request.client_writer_token == None);
    }

    #[tokio::test]
    async fn test_reconnect_read_request_writer_token_error_missing_arg() {
        let (mut my, mut their) = bufs();
        my.write_all(b"read_offset/-10\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_client_writer_token() {
        let (mut my, mut their) = bufs();

        let data = [55u8; 16];
        let token_data = ReconnectToken { data }.to_b64();
        my.write_all(b"client_writer_token/").await.unwrap();
        my.write_all(&token_data).await.unwrap();
        my.write_all(b"\0").await.unwrap();

        my.shutdown().await.unwrap();
        let request = read_reconnect_headers(&mut their).await.unwrap();

        assert!(request.read_offset == None);
        assert!(request.request_writer_token == false);
        assert!(request.client_writer_token.unwrap().data == data);
    }

    #[tokio::test]
    async fn test_reconnect_read_client_writer_token_error_missing_arg() {
        let (mut my, mut their) = bufs();

        my.write_all(b"client_writer_token\0").await.unwrap();

        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_read_multiple() {
        let (mut my, mut their) = bufs();

        let data = [55u8; 16];
        let token_data = ReconnectToken { data }.to_b64();
        my.write_all(b"client_writer_token/").await.unwrap();
        my.write_all(&token_data).await.unwrap();
        my.write_all(b"\0").await.unwrap();

        my.write_all(b"request_writer_token\0").await.unwrap();

        my.write_all(b"read_offset/100\0").await.unwrap();

        my.shutdown().await.unwrap();
        let request = read_reconnect_headers(&mut their).await.unwrap();

        assert!(request.read_offset == Some(100));
        assert!(request.request_writer_token == true);
        assert!(request.client_writer_token.unwrap().data == data);
    }

    #[tokio::test]
    async fn test_reconnect_read_multiple_same_header_is_error() {
        let (mut my, mut their) = bufs();
        let data = [55u8; 16];
        let token_data = ReconnectToken { data }.to_b64();
        my.write_all(b"client_writer_token/").await.unwrap();
        my.write_all(&token_data).await.unwrap();
        my.write_all(b"\0").await.unwrap();

        let data = [100u8; 16];
        let token_data = ReconnectToken { data }.to_b64();
        my.write_all(b"client_writer_token/").await.unwrap();
        my.write_all(&token_data).await.unwrap();
        my.write_all(b"\0").await.unwrap();

        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));

        let (mut my, mut their) = bufs();
        my.write_all(b"read_offset/100\0").await.unwrap();
        my.write_all(b"read_offset/100\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));

        let (mut my, mut their) = bufs();
        my.write_all(b"request_writer_token\0").await.unwrap();
        my.write_all(b"request_writer_token\0").await.unwrap();
        my.shutdown().await.unwrap();
        let e = read_reconnect_headers(&mut their).await.unwrap_err();
        assert!(matches!(e, ConnectionError::BadData(_)));
    }

    #[tokio::test]
    async fn test_reconnect_write_reconnect_response_empty() {
        let (mut my, mut their) = bufs();
        let request = ReconnectRequest::new();
        let response = ReconnectResponse {
            request_writer_token: None
        };
        let h = write_reconnect_response(&mut their, &request, &response).await.unwrap();
        assert_eq!(h.request_writer_token, None);
        assert_eq!(h.client_writer_token, None);
        assert_eq!(h.read_offset, None);

        their.shutdown().await.unwrap();
        let mut out: Vec<u8> = vec!();
        my.read_to_end(&mut out).await.unwrap();
        assert_eq!(&out, b"");
    }

    #[tokio::test]
    async fn test_reconnect_write_reconnect_response() {
        let (mut my, mut their) = bufs();
        let mut request = ReconnectRequest::new();
        request.request_writer_token = true;
        let t = ReconnectToken::from_b64(b"YWFhYWFhYWFhYWFhYWFhYQ==").unwrap();
        let response = ReconnectResponse {
            request_writer_token: Some(t)
        };
        let h = write_reconnect_response(&mut their, &request, &response).await.unwrap();
        assert_eq!(h.request_writer_token, Some(ReconnectToken { data: [b'a'; 16] }));
        assert_eq!(h.client_writer_token, None);
        assert_eq!(h.read_offset, None);

        their.shutdown().await.unwrap();
        let mut out: Vec<u8> = vec!();
        my.read_to_end(&mut out).await.unwrap();
        assert_eq!(&out, b"request_writer_token/YWFhYWFhYWFhYWFhYWFhYQ==\0");
    }

    #[tokio::test]
    async fn test_reconnect_write_client_writer_token() {
        let (mut my, mut their) = bufs();
        write_client_writer_token(&mut their, 125).await.unwrap();

        their.shutdown().await.unwrap();
        let mut out: Vec<u8> = vec!();
        my.read_to_end(&mut out).await.unwrap();
        assert_eq!(&out, b"client_writer_token/125\0");
    }
}
