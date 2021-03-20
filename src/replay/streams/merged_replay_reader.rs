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
        loop {
            let data_read = reader.read(&mut *buf).unwrap();
            if data_read != 0 {
                c.write_all(&buf[..data_read]).await?;
            } else {
                let f = self.replay.borrow().wait_for_more_data();
                f.await;
                let r = self.replay.borrow();
                if r.delayed_len() <= reader.position() && r.is_finished() {
                    return Ok(())
                }
            }
        }
    }
}
