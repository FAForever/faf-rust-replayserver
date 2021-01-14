use std::{cell::RefCell, rc::Rc, collections::HashMap, collections::HashSet};

use crate::replay::position::StreamPosition;

use super::{writer_replay::WriterReplay, merge_strategy::MergeStrategy, merged_replay::MergedReplay};

type WReplayRef = Rc<RefCell<WriterReplay>>;
type MReplayRef = Rc<RefCell<MergedReplay>>;

enum ReplayStatus {
    Diverged,   // Replay diverged from canonical stream.
    Reserve,    // Replay matches canonical stream so far and can be used in the future.
    Quorum,     // Stream is part of quorum used to build a canonical stream.
    StalemateCandidate(u8), // Stream is a candidate for a new quorum during a stalemate.
}

struct ReplayState {
    id: u64,
    replay: WReplayRef,
    status: ReplayStatus,
    compared_data: usize,
}

impl ReplayState {
    fn new(id: u64, replay: WReplayRef) -> Self {
        Self { id, replay, status: ReplayStatus::Reserve, compared_data: 0,}
    }
}

struct SharedState {
    token: u64,
    pub replays: HashMap<u64, ReplayState>,
    pub canonical_stream: MReplayRef,     // FIXME
    pub data_merged: usize,
    pub data_to_send: usize,
}

impl SharedState {
    fn new() -> Self {
        Self {
            token: 0,
            replays: HashMap::new(),
            canonical_stream: Rc::new(RefCell::new(MergedReplay::new())),
            data_merged: 0,
            data_to_send: 0,
        }
    }

    fn add_replay(&mut self, r: WReplayRef) -> u64 {
        let token = self.token;
        self.replays.insert(token, ReplayState::new(token, r));
        self.token += 1;
        token
    } 

    fn get_replay(&self, token: u64) -> &ReplayState {
        self.replays.get(&token).unwrap()
    }

    fn get_mut_replay(&mut self, token: u64) -> &mut ReplayState {
        self.replays.get_mut(&token).unwrap()
    }
}


struct MergeQuorumState {
    pub s: SharedState,
    quorum: HashSet<u64>,
    reserve: HashSet<u64>,
    quorum_diverges_at: Option<usize>,
}

struct MergeStalemateState {
    pub s: SharedState,
    candidates: HashMap<u8, Vec<u64>>,
    reserve: HashSet<u64>,
}

impl MergeQuorumState {
    fn has_to_enter_stalemate(&self) -> bool {
        self.quorum_diverges_at.map_or(false, |end| end <= self.s.data_to_send)
    }

    fn add_replay(&mut self, r: WReplayRef) -> u64 {
        let token = self.s.add_replay(r);
        self.reserve.insert(token);
        token
    }

    fn enter_stalemate(self) -> MergeStalemateState {
        todo!()
    }
}

impl MergeStalemateState {
    fn new() -> Self {
        Self {
            s: SharedState::new(),
            candidates: HashMap::new(),
            reserve: HashSet::new(),
        }
    }

    fn can_exit_stalemate(&self) -> bool {
        todo!()
    }

    fn exit_stalemate(self) -> MergeQuorumState {
        todo!()
    }

    fn add_replay(&mut self, r: WReplayRef) -> u64 {
        let token = self.s.add_replay(r);
        self.reserve.insert(token);
        token
    }
}


enum QuorumMergeStrategy {
    Quorum(MergeQuorumState),
    Stalemate(MergeStalemateState),
}

macro_rules! both {
    ($value:expr, $pattern:pat => $result:expr) => (
        match $value {
            Self::Quorum($pattern) => $result,
            Self::Stalemate($pattern) => $result,
        }
    )
}

impl QuorumMergeStrategy {
    pub fn new() -> Self {
        Self::Stalemate(MergeStalemateState::new())
    }

    fn should_change_status(&self) -> bool {
        match &self {
            Self::Quorum(s) => s.has_to_enter_stalemate(),
            Self::Stalemate(s) => s.can_exit_stalemate(),
        }
    }
}

impl MergeStrategy for QuorumMergeStrategy {
    fn replay_added(&mut self, r: WReplayRef) -> u64 {
        both!(self, s => s.add_replay(r))
    }

    fn replay_removed(&mut self, id: u64) {
        todo!()
    }

    fn replay_header_added(&mut self, id: u64) {
        let replay = both!(self, s => s.s.get_replay(id));
        let header = replay.replay.borrow_mut().take_header();
        let mut canonical_stream = both!(self, s => s.s.canonical_stream.borrow_mut());
        if canonical_stream.position() < StreamPosition::HEADER {
            canonical_stream.add_header(header);
        }
    }

    fn replay_new_data(&mut self, id: u64) {
        todo!()
    }

    fn replay_new_delayed_data(&mut self, id: u64, data_len: usize) {
        todo!()
    }

    fn get_merged_replay(&self) -> MReplayRef {
        both!(self, s => s.s.canonical_stream.clone())
    }
}
