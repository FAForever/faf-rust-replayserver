use std::{cell::RefCell, rc::Rc};

use futures::Future;
use tokio::{io::AsyncReadExt, select};
use tokio_util::sync::CancellationToken;

use crate::{replay::{header::ReplayHeader, streams::position::StreamPosition}, server::connection::Connection, replay::streams::position::PositionTracker, error::ConnResult, async_utils::buflist::BufList, async_utils::buf_with_discard::BufWithDiscard, async_utils::buf_with_discard::ReadAt};

enum MaybeHeader {
    None,
    Some(ReplayHeader),
    Discarded(usize),
}

pub struct WriterReplay {
    header: MaybeHeader,
    data: BufList,
    progress: PositionTracker,
}

impl WriterReplay {
    pub fn new() -> Self {
        Self {
            header: MaybeHeader::None,
            data: BufList::new(),
            progress: PositionTracker::new(),
        }
    }
    pub fn add_header(&mut self, h: ReplayHeader) {
        debug_assert!(self.progress.position() == StreamPosition::START);
        self.header = MaybeHeader::Some(h);
        self.progress.advance(StreamPosition::DATA(0));
    }

    pub fn take_header(&mut self) -> ReplayHeader {
        let h = match self.header {
            MaybeHeader::Some(h) => h,
            _ => panic!("Cannot take header"),
        };
        self.header = MaybeHeader::Discarded(h.data.len());
        h
    }

    pub fn add_data(&mut self, buf: &[u8]) {
        debug_assert!(self.position() >= StreamPosition::DATA(0));
        debug_assert!(self.position() < StreamPosition::FINISHED(0));
        self.data.append(buf);
        self.progress.advance(self.position() + buf.len());
    }

    pub fn get_mut_data(&mut self) -> &mut impl BufWithDiscard {
        &mut self.data
    }

    pub fn finish(&mut self) {
        let final_len = self.position().len();
        self.progress.advance(StreamPosition::FINISHED(final_len));
    }

    pub fn wait(&self, until: StreamPosition) -> impl Future<Output = StreamPosition> {
        self.progress.wait(until)
    }

    pub fn position(&self) -> StreamPosition {
        self.progress.position()
    }
}

impl ReadAt for WriterReplay {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        // Unused. Merged replay will have a data structure that doesn't discard.
        todo!()
/*
        match &self.header {
            None => return Ok(0),
            Some(h) => {
                if start >= h.data.len() {
                    self.data.read_at(start - h.data.len(), buf)
                } else {
                    let mut hb = &h.data[start..];
                    <&[u8] as Read>::read(&mut hb, buf)
                }
            }
        }
*/
    }
}

pub async fn read_from_connection(me: Rc<RefCell<WriterReplay>>, c: &mut Connection, shutdown_token: CancellationToken) {
    select! {
        // Ignore connection errors. We only need the data.
        _ = do_read_from_connection(&me, c) => {}
        _ = shutdown_token.cancelled() => {},
    }
    me.borrow_mut().finish();
}

async fn do_read_from_connection(me: &Rc<RefCell<WriterReplay>>, mut c: &mut Connection) -> ConnResult<()> {
    // TODO dep injection?
    let header = ReplayHeader::from_connection(&mut c).await?;
    {
        me.borrow_mut().add_header(header);
    }
    /* We can't modify inner.data in-place, merging might use it in the meantime
     * Maybe we could use a different structure? Can't be bothered rn
     * */
    let mut buf: Box<[u8]> = Box::new([0; 4096]);
    loop {
        let read = c.read(&mut *buf).await?;
        if read == 0 {
            break
        }
        {
            me.borrow_mut().add_data(&buf[0..read]);
        }
    }
    Ok(())
}
