use tokio::{io::BufReader, net::TcpStream, io::AsyncRead, io::AsyncBufRead, io::AsyncWrite, io::AsyncBufReadExt};
use crate::accept::header::ConnectionHeader;

pub type ReaderType = Box<dyn AsyncBufRead + Send>;
pub type WriterType = Box<dyn AsyncWrite + Send>;

pub struct Connection
{
    // FIXME. I *hate* to use boxed traits here, but with async traits below there really isn't a
    // better way to mock this. If it ends up hurting performance, we can conditionally replace it
    // with a non-box.
    //
    // reader: BufReader<OwnedReadHalf>,
    // writer: OwnedWriteHalf,
    reader: ReaderType,
    writer: WriterType,
    header: Option<ConnectionHeader>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        let (r, w) = stream.into_split();
        let reader = Box::new(BufReader::new(r));
        let writer = Box::new(w);
        Self {reader, writer, header: None }
    }

    // Yes, test-specific code. Connection is just a wrapper for a few things, so I think it's
    // justified.
    #[cfg(test)]
    pub fn test(reader: ReaderType, writer: WriterType) -> Self {
        Self { reader, writer, header: None }
    }

    pub fn set_header(&mut self, header: ConnectionHeader)
    {
        self.header = Some(header);
    }

    pub fn get_header(&self) -> ConnectionHeader
    {
        self.header.clone().unwrap()
    }

    fn get_buf_reader(&mut self) -> &mut dyn AsyncBufRead {
        &mut *self.reader
    }
    fn get_writer(&mut self) -> &mut dyn AsyncWrite {
        &mut *self.writer
    }
}

// Boilerplate impls start
impl AsyncRead for Connection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut().get_buf_reader()) };
        AsyncRead::poll_read(r, cx, buf)
    }
}

impl AsyncBufRead for Connection {
    fn poll_fill_buf(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<&[u8]>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut().get_buf_reader() ) };
        AsyncBufRead::poll_fill_buf(r, cx)
    }

    fn consume(self: std::pin::Pin<&mut Self>, amt: usize) {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut().get_buf_reader() ) };
        AsyncBufRead::consume(r, amt)
    }
}

impl AsyncWrite for Connection {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut().get_writer() ) };
        AsyncWrite::poll_write(r, cx, buf)
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut().get_writer() ) };
        AsyncWrite::poll_flush(r, cx)
    }

    fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
        let r = unsafe {std::pin::Pin::new_unchecked( self.get_unchecked_mut().get_writer() ) };
        AsyncWrite::poll_shutdown(r, cx)
    }
}
// Boilerplate impls end

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
