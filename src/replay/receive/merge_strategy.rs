use std::{cell::RefCell, rc::Rc};

use futures::StreamExt;

use crate::replay::streams::position::StreamPosition;

use super::{writer_replay::WriterReplay, replay_delay::StreamDelay};

type ReplayRef = Rc<RefCell<WriterReplay>>;

pub trait MergeStrategy {
    /* We use IDs to identify replays for simplicity, */
    fn replay_added(&mut self, w: Rc<RefCell<WriterReplay>>) -> u64;
    fn replay_removed(&mut self, id: u64);
    fn replay_header_added(&mut self, id: u64);
    fn replay_new_data(&mut self, id: u64);
    fn replay_new_delayed_data(&mut self, id: u64, data_len: usize);
    fn get_merged_replay(&mut self) -> Rc<RefCell<WriterReplay>>;   // FIXME change return type?
}

// TODO
pub struct NullMergeStrategy {
}

impl MergeStrategy for NullMergeStrategy {
    fn replay_added(&mut self, w: ReplayRef) -> u64 {
        0
    }

    fn replay_removed(&mut self, id: u64) {
    }

    fn replay_header_added(&mut self, id: u64) {
    }

    fn replay_new_data(&mut self, id: u64) {
    }

    fn replay_new_delayed_data(&mut self, id: u64, data_len: usize) {
    }

    fn get_merged_replay(&mut self) -> ReplayRef {
        Rc::new(RefCell::new(WriterReplay::new()))
    }
}

pub async fn track_replay(s: &RefCell<impl MergeStrategy>, delay: &StreamDelay, r: ReplayRef) {
    let token = s.borrow_mut().replay_added(r.clone());
    delay.delayed_progress(&*r).for_each( |positions| async move {
        let mut st = s.borrow_mut();

        if positions.current == StreamPosition::HEADER {
            st.replay_header_added(token);
        } else if matches!(positions.current, StreamPosition::FINISHED(_)) {
            st.replay_removed(token);
            return
        } else {
            let delayed_len = positions.delayed.len();
            st.replay_new_data(token);
            st.replay_new_delayed_data(token, delayed_len);
        }

    }).await;
    s.borrow_mut().replay_removed(token);
}
