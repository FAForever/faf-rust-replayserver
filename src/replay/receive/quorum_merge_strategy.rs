use std::{cell::RefCell, rc::Rc, collections::HashMap, collections::HashSet};

use crate::replay::streams::position::StreamPosition;

use super::{writer_replay::WriterReplay, merge_strategy::MergeStrategy};

enum ReplayStatus {
    Diverged,   // Replay diverged from canonical stream.
    Reserve,    // Replay matches canonical stream so far and can be used in the future.
    Quorum,     // Stream is part of quorum used to build a canonical stream.
    StalemateCandidate(u8), // Stream is a candidate for a new quorum during a stalemate.
}

struct ReplayState {
    id: u64,
    replay: Rc<RefCell<WriterReplay>>,
    status: ReplayStatus,
    compared_data: usize,
}

impl ReplayState {
    fn new(id: u64, replay: Rc<RefCell<WriterReplay>>) -> Self {
        Self { id, replay, status: ReplayStatus::Reserve, compared_data: 0 }
    }
}

enum MergeType {
    Quorum {
        quorum: HashSet<u64>,
    },
    Stalemate {
        candidates: HashMap<u8, Vec<u64>>,
    }
}

struct MergingState {
    type_: MergeType,
    reserve: HashSet<u64>,
    data_merged: usize,
    data_sent: usize,
}

impl MergingState {
    pub fn new() -> Self {
        Self {
            type_: MergeType::Stalemate {
                candidates: HashMap::new(),
            },
            reserve: HashSet::new(),
            data_merged: 0,
            data_sent: 0,
        }
    }

    pub fn should_change_state() {
        todo!();
    }

    pub fn replay_added(&mut self, id: u64) {
        self.reserve.insert(id);
        // New streams start empty, no state changes needed.
    }
}

pub struct QuorumMergeStrategy {
    token: u64,
    replays: HashMap<u64, ReplayState>,
    state: MergingState,
    canonical_stream: WriterReplay,     // FIXME
}

impl QuorumMergeStrategy {
    fn new() -> Self {
        Self {
            token: 0,
            replays: HashMap::new(),
            state: MergingState::new(),
            canonical_stream: WriterReplay::new(),
        }
    }

    fn get_replay(&self, token: u64) -> &ReplayState {
        self.replays.get(&token).unwrap()
    }

    fn get_mut_replay(&mut self, token: u64) -> &mut ReplayState {
        self.replays.get_mut(&token).unwrap()
    }
}

impl MergeStrategy for QuorumMergeStrategy {
    fn replay_added(&mut self, w: Rc<RefCell<WriterReplay>>) -> u64 {
        let token = self.token;
        self.replays.insert(token, ReplayState::new(token, w));
        self.state.replay_added(token);
        self.token += 1;
        token
    }

    fn replay_removed(&mut self, id: u64) {
        todo!()
    }

    fn replay_header_added(&mut self, id: u64) {
        let replay = self.get_replay(id);
        let header = replay.replay.borrow_mut().take_header();
        if self.canonical_stream.position() < StreamPosition::HEADER {
            self.canonical_stream.add_header(header);
        }
    }

    fn replay_new_data(&mut self, id: u64) {
        todo!()
    }

    fn replay_new_delayed_data(&mut self, id: u64, data_len: usize) {
        todo!()
    }

    fn get_merged_replay(&mut self) -> Rc<RefCell<WriterReplay>> {
        todo!()
    }
}
