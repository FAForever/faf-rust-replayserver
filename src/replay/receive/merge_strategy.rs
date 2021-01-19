use std::{cell::RefCell, rc::Rc};

use futures::StreamExt;

use crate::replay::{position::StreamPosition, streams::WriterReplay, streams::MergedReplay};

use super::replay_delay::StreamDelay;

type ReplayRef = Rc<RefCell<WriterReplay>>;

// This represents a way to merge replays into one canonical replay. We define:
// * A set of replays R.
//   * A replay has a header, data and a delayed position in the data. A header can be added, data
//     can be appended, delayed position can increase.
//   * The strategy is called when a replay is added, finished, a replay's header is added, a
//     replay's data is added or its delayed position is moved. Note that, because of task
//     scheduler, we can only guarantee that e.g. replay does have a header, or did finish. At each
//     call, state of every replay could've advanced arbitrarily.
//   * A replay is first added, then its header is added, then its data and delayed data change,
//     then it is marked as finished.
//   * Before calling replay_removed(), replay_new_data() and replay_new_delayed_data() will be
//     called one last time with all replay data present.
//   * FIXME: Replay's delayed position in the data is passed in as an argument, not kept within the
//     replay. Perhaps it should be. For now we have to store it if we need it.
//  * A canonical replay C.
//    * The strategy sets C's header, writes data to C, sets C's delayed position and finishes C
//      based on R.
//    * Details are left to the strategy. Some obvious invariants have to be kept - adding the
//      header first, not moving delayed position beyond real data, not inventing data from thin
//      air and not doing anything after finishing C.
//  * After all replays are added, processed and removed, finish() is called. At finish(),
//    strategy should merge all outstanding data.
//
//    ##########################
//    #         CAVEAT         #
//    ##########################
//    The quorum merge strategy relies on new_data and new_delayed_data being throttled to once a
//    second to avoid some (very rare) pathological performance issues. Don't change that too much.
pub trait MergeStrategy {
    /* We use IDs to identify replays for simplicity, */
    fn replay_added(&mut self, w: Rc<RefCell<WriterReplay>>) -> u64;
    fn replay_removed(&mut self, id: u64);
    fn replay_header_added(&mut self, id: u64);
    fn replay_new_data(&mut self, id: u64);
    fn replay_new_delayed_data(&mut self, id: u64, data_len: usize);
    fn finish(&mut self);
    fn get_merged_replay(&self) -> Rc<RefCell<MergedReplay>>;   // FIXME change return type?
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

    fn get_merged_replay(&self) -> Rc<RefCell<MergedReplay>> {
        todo!()
    }
    fn finish(&mut self) {
        todo!()
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
        } else {
            let delayed_len = positions.delayed.len();
            st.replay_new_data(token);
            st.replay_new_delayed_data(token, delayed_len);
        }

    }).await;
    s.borrow_mut().replay_removed(token);
}
