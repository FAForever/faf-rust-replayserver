use std::io::Read;

use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{replay::streams::MReplayRef, util::buf_traits::ReadAtExt};

pub struct MergedReplayReader {
    replay: MReplayRef,
}

impl MergedReplayReader {
    pub fn new(replay: MReplayRef) -> Self {
        Self { replay }
    }

    pub async fn write_to<T: AsyncWrite + Unpin>(&mut self, c: &mut T) -> std::io::Result<()> {
        let mut buf: Box<[u8]> = Box::new([0; 4096]);
        let mut reader = self.replay.reader();
        let mut wrote_after_wait = true;
        loop {
            let data_read = reader.read(&mut *buf).unwrap();
            if data_read != 0 {
                c.write_all(&buf[..data_read]).await?;
                wrote_after_wait = true;
            } else if wrote_after_wait {
                let f = self.replay.borrow().wait_for_more_data();
                f.await;
                wrote_after_wait = false;
            } else {
                return Ok(());
            }
        }
    }
}
