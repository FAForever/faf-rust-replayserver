use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite};

pub struct BufReadWrite<R: AsyncBufRead + Send, W: AsyncWrite + Send> {
    r: R,
    w: W,
}

impl<R: AsyncBufRead + Send, W: AsyncWrite + Send> BufReadWrite<R, W> {
    pub fn new(r: R, w: W) -> Self {
        Self {r, w}
    }
}


// Boilerplate impls start
impl<R: AsyncBufRead + Send, W: AsyncWrite + Send> AsyncRead for BufReadWrite<R, W> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let r = unsafe { std::pin::Pin::new_unchecked(&mut self.get_unchecked_mut().r) };
        AsyncRead::poll_read(r, cx, buf)
    }
}

impl<R: AsyncBufRead + Send, W: AsyncWrite + Send> AsyncBufRead for BufReadWrite<R, W> {
    fn poll_fill_buf(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<&[u8]>> {
        let r = unsafe { std::pin::Pin::new_unchecked(&mut self.get_unchecked_mut().r) };
        AsyncBufRead::poll_fill_buf(r, cx)
    }

    fn consume(self: std::pin::Pin<&mut Self>, amt: usize) {
        let r = unsafe { std::pin::Pin::new_unchecked(&mut self.get_unchecked_mut().r) };
        AsyncBufRead::consume(r, amt)
    }
}

impl<R: AsyncBufRead + Send, W: AsyncWrite + Send> AsyncWrite for BufReadWrite<R, W> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let r = unsafe { std::pin::Pin::new_unchecked(&mut self.get_unchecked_mut().w) };
        AsyncWrite::poll_write(r, cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let r = unsafe { std::pin::Pin::new_unchecked(&mut self.get_unchecked_mut().w) };
        AsyncWrite::poll_flush(r, cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let r = unsafe { std::pin::Pin::new_unchecked(&mut self.get_unchecked_mut().w) };
        AsyncWrite::poll_shutdown(r, cx)
    }
}
// Boilerplate impls end
