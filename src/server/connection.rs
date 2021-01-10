use tokio::{io::BufReader, net::TcpStream, io::AsyncRead, io::AsyncBufRead, io::AsyncWrite, io::AsyncBufReadExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use crate::accept::header::ConnectionHeader;

#[cfg_attr(soon, faux::create)]
pub struct Connection
{
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    header: Option<ConnectionHeader>,
}

#[cfg_attr(soon, faux::methods)]
impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        let (r, writer) = stream.into_split();
        let reader = BufReader::new(r);
        Self {reader, writer, header: None }
    }
    pub fn set_header(&mut self, header: ConnectionHeader)
    {
        self.header = Some(header);
    }

    pub fn get_header(&self) -> ConnectionHeader
    {
        // We always unwrap anyway
        self.header.clone().unwrap()
    }
}

// Boilerplate impls start
impl AsyncRead for Connection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut()) };
        AsyncRead::poll_read(r, cx, buf)
    }
}

impl AsyncBufRead for Connection {
    fn poll_fill_buf(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<&[u8]>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut()) };
        AsyncBufRead::poll_fill_buf(r, cx)
    }

    fn consume(self: std::pin::Pin<&mut Self>, amt: usize) {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut()) };
        AsyncBufRead::consume(r, amt)
    }
}

impl AsyncWrite for Connection {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut()) };
        AsyncWrite::poll_write(r, cx, buf)
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut()) };
        AsyncWrite::poll_flush(r, cx)
    }

    fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut()) };
        AsyncWrite::poll_shutdown(r, cx)
    }
}
// Boilerplate impls start

// FIXME should be a trait at some point
pub async fn read_until_exact<T: AsyncBufRead + Unpin>(r: &mut T, byte: u8, buf: &mut Vec<u8>) -> std::io::Result<usize>
{
    let result = r.read_until(byte, buf).await?;
    if buf[buf.len() - 1] != byte {
        Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("Reached end of stream while reading until '{}'", byte)))
    } else {
        Ok(result)
    }
}

#[cfg(soon)]
pub mod test {
    use std::{io::{Cursor, Read, BufRead}, sync::Arc, sync::Mutex, io::Write, io::Seek, io::SeekFrom};
    use crate::{error::{ConnResult, ConnectionError}, accept::header::ConnectionHeader};
    use super::Connection;

    /* We can't reuse the regular connection because you can't pass async fns to faux yet */
    pub struct TestConnection {
        stream: Cursor<Vec<u8>>,
        header: Option<ConnectionHeader>,
    }
    impl TestConnection {
        pub fn new() -> Self {
            let v = Cursor::new(Vec::new());
            Self { stream: v, header: None}
        }

        pub fn read_exact(&mut self, buf: &mut[u8]) -> ConnResult<()>
        {
            self.stream.read_exact(buf).map_err(ConnectionError::from)
        }

        pub fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>, limit: u64) -> ConnResult<usize>
        {
            Read::by_ref(&mut self.stream).take(limit).read_until(byte, buf).map_err(ConnectionError::from)
        }

        pub fn set_header(&mut self, header: ConnectionHeader)
        {
            self.header = Some(header);
        }

        pub fn get_header(&self) -> ConnectionHeader
        {
            self.header.unwrap()
        }

        pub fn append_read_data(&mut self, buf: &[u8])
        {
            let rpos = self.stream.position();
            self.stream.seek(SeekFrom::End(0)).unwrap();
            self.stream.write(buf).unwrap();
            self.stream.seek(SeekFrom::Start(rpos)).unwrap();
        }

        pub fn faux() -> (Arc<Mutex<TestConnection>>, Connection) {
            let tc = Arc::new(Mutex::new(TestConnection::new()));
            let mut c = Connection::faux();
            unsafe {
                let tcc = tc.clone();
                faux::when!(c.read_exact).then(move |buf| tcc.lock().unwrap().read_exact(buf));
                let tcc = tc.clone();
                faux::when!(c.read_until).then(move |(byte, buf, limit)| tcc.lock().unwrap().read_until(byte, buf, limit));
                let tcc = tc.clone();
                faux::when!(c.get_header).then(move |()| tcc.lock().unwrap().get_header());
                let tcc = tc.clone();
                faux::when!(c.set_header).then(move |h| tcc.lock().unwrap().set_header(h));
            }
            (tc, c)
        }
    }
}
