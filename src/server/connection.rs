use rand::Rng;
use std::fmt::Display;

use crate::{
    accept::header::{ConnectionHeader, ConnectionType},
    metrics,
};
use tokio::{io::AsyncBufRead, io::AsyncBufReadExt, io::AsyncRead, io::AsyncWrite, io::BufReader, net::TcpStream};

pub type ReaderType = Box<dyn AsyncBufRead + Send>;
pub type WriterType = Box<dyn AsyncWrite + Send>;

pub struct Connection {
    reader: ReaderType,
    writer: WriterType,
    header: Option<ConnectionHeader>,
    id: String,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        let (r, w) = stream.into_split();
        let reader = Box::new(BufReader::new(r));
        let writer = Box::new(w);
        Self::new_from(reader, writer)
    }

    pub fn new_from(reader: ReaderType, writer: WriterType) -> Self {
        let mut id = String::new();
        for _ in 0..12 {
            id.push(rand::rng().random_range('a'..'z'));
        }
        let s = Self {
            reader,
            writer,
            header: None,
            id,
        };
        s.set_metric();
        log::debug!("New {}", s);
        s
    }

    fn header2label(&self) -> &str {
        match &self.header {
            None => "initial",
            Some(h) => match h.type_ {
                ConnectionType::Reader => "reader",
                ConnectionType::Writer => "writer",
            },
        }
    }

    fn set_metric(&self) {
        metrics::ACTIVE_CONNS.with_label_values(&[self.header2label()]).inc();
    }
    fn reset_metric(&self) {
        metrics::ACTIVE_CONNS.with_label_values(&[self.header2label()]).dec();
    }

    pub fn set_header(&mut self, header: ConnectionHeader) {
        self.reset_metric();
        self.header = Some(header);
        self.set_metric();
    }

    pub fn get_header(&self) -> ConnectionHeader {
        self.header.clone().unwrap()
    }

    fn get_buf_reader(&mut self) -> &mut dyn AsyncBufRead {
        &mut *self.reader
    }
    fn get_writer(&mut self) -> &mut dyn AsyncWrite {
        &mut *self.writer
    }
}

impl Display for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.header {
            None => write!(f, "Connection {}", self.id),
            Some(h) => write!(
                f,
                "Connection {}, {} '{}' for replay {}",
                self.id,
                match h.type_ {
                    ConnectionType::Reader => "reader",
                    ConnectionType::Writer => "writer",
                },
                h.name,
                h.id
            ),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.reset_metric();
        log::debug!("{} ended", self);
    }
}

// Boilerplate impls start
impl AsyncRead for Connection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let r = unsafe { std::pin::Pin::new_unchecked(self.get_unchecked_mut().get_buf_reader()) };
        AsyncRead::poll_read(r, cx, buf)
    }
}

impl AsyncBufRead for Connection {
    fn poll_fill_buf(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<&[u8]>> {
        let r = unsafe { std::pin::Pin::new_unchecked(self.get_unchecked_mut().get_buf_reader()) };
        AsyncBufRead::poll_fill_buf(r, cx)
    }

    fn consume(self: std::pin::Pin<&mut Self>, amt: usize) {
        let r = unsafe { std::pin::Pin::new_unchecked(self.get_unchecked_mut().get_buf_reader()) };
        AsyncBufRead::consume(r, amt)
    }
}

impl AsyncWrite for Connection {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let r = unsafe { std::pin::Pin::new_unchecked(self.get_unchecked_mut().get_writer()) };
        AsyncWrite::poll_write(r, cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let r = unsafe { std::pin::Pin::new_unchecked(self.get_unchecked_mut().get_writer()) };
        AsyncWrite::poll_flush(r, cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let r = unsafe { std::pin::Pin::new_unchecked(self.get_unchecked_mut().get_writer()) };
        AsyncWrite::poll_shutdown(r, cx)
    }
}
// Boilerplate impls end

// FIXME should be a trait at some point
pub async fn read_until_exact<T: AsyncBufRead + Unpin>(
    r: &mut T,
    byte: u8,
    buf: &mut Vec<u8>,
) -> std::io::Result<usize> {
    let result = r.read_until(byte, buf).await?;
    if (result == 0) || (buf[buf.len() - 1] != byte) {
        Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            format!("Reached end of stream while reading until '{}'", byte),
        ))
    } else {
        Ok(result)
    }
}

#[cfg(test)]
pub mod test {
    use tokio::{io::AsyncWriteExt, io::BufReader, join};

    use super::*;

    #[tokio::test]
    async fn read_until_exact_normal() {
        let (mut i, o) = tokio::io::duplex(1024);
        let mut buf_read = BufReader::new(o);

        let writing_data = async move {
            let buf: &[u8] = &[1, 2, 3, 4, 5];
            i.write_all(buf).await.unwrap();
            drop(i);
        };
        let read_header = async {
            let mut data = vec![];
            let val = read_until_exact(&mut buf_read, 3, &mut data).await.unwrap();
            assert_eq!(val, 3);
            assert_eq!(data, vec!(1, 2, 3));
        };
        join! {
            writing_data,
            read_header,
        };
    }

    #[tokio::test]
    async fn read_until_exact_missing() {
        let (mut i, o) = tokio::io::duplex(1024);
        let mut buf_read = BufReader::new(o);

        let writing_data = async move {
            let buf: &[u8] = &[1, 2, 3, 4, 5];
            i.write_all(buf).await.unwrap();
            drop(i);
        };
        let read_header = async {
            let mut data = vec![];
            read_until_exact(&mut buf_read, 8, &mut data)
                .await
                .expect_err("Should've reached end without finding 8");
        };
        join! {
            writing_data,
            read_header,
        };
    }

    #[tokio::test]
    async fn read_until_exact_empty() {
        let (i, o) = tokio::io::duplex(1024);
        let mut buf_read = BufReader::new(o);

        let writing_data = async move {
            drop(i);
        };
        let read_header = async {
            let mut data = vec![];
            let _name = read_until_exact(&mut buf_read, 0, &mut data)
                .await
                .expect_err("Zero read should be short");
        };
        join! {
            writing_data,
            read_header,
        };
    }

    /* Connection, reader, writer */
    pub type MockConnection = (Connection, tokio::io::DuplexStream, tokio::io::DuplexStream);
    pub fn test_connection() -> MockConnection {
        // NOTE: setting too small buffer sizes below interacts badly with tokio::time::pause.
        // As long as they're larger than the largest possible read, everything seems OK.
        let (reader, conn_writer) = tokio::io::duplex(10240);
        let (conn_reader, writer) = tokio::io::duplex(10240);
        let c = Connection::new_from(Box::new(BufReader::new(conn_reader)), Box::new(conn_writer));
        (c, reader, writer)
    }
}
