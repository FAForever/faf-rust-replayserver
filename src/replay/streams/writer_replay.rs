use std::{cell::RefCell, io::Write, rc::Rc};

use tokio::io::AsyncReadExt;

use crate::{
    error::ConnResult, server::connection::Connection, util::buf_deque::BufDeque, util::buf_traits::ChunkedBuf,
};

use super::ReplayHeader;

pub struct WriterReplay {
    header: Option<ReplayHeader>,
    data: BufDeque,
    delayed_data_len: usize,
    finished: bool,
}

impl WriterReplay {
    pub fn new() -> Self {
        Self {
            header: None,
            data: BufDeque::new(),
            delayed_data_len: 0,
            finished: false,
        }
    }

    pub fn add_header(&mut self, h: ReplayHeader) {
        self.header = Some(h);
    }

    pub fn take_header(&mut self) -> ReplayHeader {
        std::mem::replace(&mut self.header, None).expect("Cannot take header")
    }

    pub fn add_data(&mut self, buf: &[u8]) {
        self.data.write_all(buf).unwrap();
    }

    pub fn get_data(&self) -> &impl ChunkedBuf {
        &self.data
    }

    pub fn set_delayed_data_len(&mut self, new: usize) {
        debug_assert!(self.delayed_data_len <= new);
        self.delayed_data_len = new;
    }

    pub fn get_delayed_data_len(&self) -> usize {
        self.delayed_data_len
    }

    pub fn discard(&mut self, until: usize) {
        self.data.discard(until);
    }

    pub fn discard_all(&mut self) {
        self.data.discard(usize::MAX);
    }

    pub fn finish(&mut self) {
        self.finished = true;
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

pub type WReplayRef = Rc<RefCell<WriterReplay>>;

pub async fn read_header(me: WReplayRef, c: &mut Connection) -> ConnResult<()> {
    let header = ReplayHeader::from_connection(c).await?;
    me.borrow_mut().add_header(header);
    Ok(())
}

pub async fn read_data(me: WReplayRef, c: &mut Connection) -> ConnResult<()> {
    let mut buf: Box<[u8]> = Box::new([0; 4096]);
    loop {
        let read = c.read(&mut *buf).await?;
        if read == 0 {
            break;
        }
        me.borrow_mut().add_data(&buf[0..read]);
    }
    Ok(())
}
