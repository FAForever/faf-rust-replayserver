use std::{fs::File, io::Read, path::PathBuf, sync::Once};

use time::{Date, OffsetDateTime, Time};
use tokio::time::Duration;

static LOG: Once = Once::new();

pub fn setup_logging() {
    LOG.call_once(env_logger::init);
}

pub fn get_file(f: &str) -> Vec<u8> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("test/resources");
    p.push(f);
    let mut res = Vec::new();
    File::open(p).unwrap().read_to_end(&mut res).unwrap();
    res
}

pub fn compare_bufs(br1: impl AsRef<[u8]>, br2: impl AsRef<[u8]>) {
    let b1: &[u8] = br1.as_ref();
    let b2: &[u8] = br2.as_ref();
    if b1.len() != b2.len() {
        panic!(
            "Length mismatch: {} != {}",
            b1.len(),
            b2.len()
        )
    }
    for (i, (c1, c2)) in b1
        .iter()
        .zip(b2.iter())
        .enumerate()
    {
        if c1 != c2 {
            panic!("Buffers differ at byte {}: {} != {}", i, c1, c2);
        }
    }
}

pub fn dt(d: Date, t: Time) -> OffsetDateTime {
    d.with_time(t).assume_utc()
}

pub async fn sleep_s(s: u64) {
    tokio::time::sleep(Duration::from_secs(s)).await;
}
