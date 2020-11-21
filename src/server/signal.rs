use stop_token::StopSource;
use std::os::unix::net::UnixStream;
use async_std::prelude::*;
use async_std::os::unix::net::UnixStream as AsyncUnixStream;
use signal_hook;
use log::debug;

pub async fn hold_until_signal(token: StopSource) {
    let (sread, swrite) = UnixStream::pair().unwrap();  /* FIXME log */
    let mut async_sread: AsyncUnixStream = sread.into();
    signal_hook::pipe::register(signal_hook::SIGINT, swrite).unwrap(); /* FIXME log */
    let mut buf = [0];
    async_sread.read_exact(&mut buf).await.unwrap(); /* FIXME log */
    debug!("Received SIGINT, dropping token");
    drop(token);
}
