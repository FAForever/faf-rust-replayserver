use signal_hook::consts::signal::SIGINT;
use std::os::unix::net::UnixStream;
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream as AsyncUnixStream;
use tokio_util::sync::CancellationToken;

pub async fn cancel_at_sigint(token: CancellationToken) {
    let (r, w) = UnixStream::pair().unwrap(); /* FIXME log */
    r.set_nonblocking(true).unwrap();
    let mut async_read = AsyncUnixStream::from_std(r).unwrap();
    signal_hook::low_level::pipe::register(SIGINT, w).unwrap();
    let mut buf: [u8; 1] = [0];
    async_read.read_exact(&mut buf).await.unwrap();
    token.cancel();
}
