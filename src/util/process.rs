// This file has some stuff we do with Rust runtime and the process that's impractical to test with
// cargo test. We build and test separate executables for this and ignore this file for coverage.

// TODO prometheus server setup.
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use std::os::unix::net::UnixStream;
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream as AsyncUnixStream;

pub fn setup_process_exit_on_panic() {
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        std::process::exit(1);
    }));
}

pub async fn wait_for_signals() {
    let (r, w) = UnixStream::pair().unwrap();
    r.set_nonblocking(true).unwrap();
    let mut async_read = AsyncUnixStream::from_std(r).unwrap();
    signal_hook::low_level::pipe::register(SIGINT, w.try_clone().unwrap()).unwrap();
    signal_hook::low_level::pipe::register(SIGTERM, w).unwrap();
    let mut buf: [u8; 1] = [0];
    async_read.read_exact(&mut buf).await.unwrap();
}
