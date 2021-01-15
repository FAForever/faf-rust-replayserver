use std::{cell::RefCell, rc::Rc, collections::HashMap, collections::HashSet};

use crate::{replay::position::StreamPosition, async_utils::buf_traits::DiscontiguousBuf, async_utils::buf_traits::DiscontiguousBufExt};

use super::{writer_replay::WriterReplay, merge_strategy::MergeStrategy, merged_replay::MergedReplay};

type WReplayRef = Rc<RefCell<WriterReplay>>;
type MReplayRef = Rc<RefCell<MergedReplay>>;

struct ReplayState {
    id: u64,
    replay: WReplayRef,
    compared_data: usize,
    // TODO need to hold delayed data progress so we can transmit the right amount after choosing
    // a new quorum.
}

impl ReplayState {
    pub fn new(id: u64, replay: WReplayRef) -> Self {
        Self { id, replay, compared_data: 0,}
    }

    pub fn discard(&mut self) {
        self.replay.borrow_mut().discard_all();
    }

    pub fn data_len(&self) -> usize {
        self.replay.borrow().get_data().len()
    }

    pub fn common_prefix_from(&self, other: &ReplayState, start: usize) -> usize {
        self.replay.borrow().get_data().common_prefix_from(other.replay.borrow().get_data(), start)
    }

    pub fn common_prefix_with_canonical_stream(&self, other: &MergedReplay, start: usize) -> usize {
        self.replay.borrow().get_data().common_prefix_from(other.get_data(), start)
    }
}

struct SharedState {
    token: u64,
    stream_cmp_distance: usize,
    pub target_quorum_size: usize,
    pub replays: HashMap<u64, ReplayState>,
    pub canonical_stream: MReplayRef,     // FIXME
    pub data_merged: usize,
    pub data_to_send: usize,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            token: 0,
            stream_cmp_distance: 4096, /* TODO configure */
            target_quorum_size: 2, /* TODO configure */
            replays: HashMap::new(),
            canonical_stream: Rc::new(RefCell::new(MergedReplay::new())),
            data_merged: 0,
            data_to_send: 0,
        }
    }

    pub fn add_replay(&mut self, r: WReplayRef) -> u64 {
        let token = self.token;
        self.replays.insert(token, ReplayState::new(token, r));
        self.token += 1;
        token
    }

    pub fn has_replay(&self, token: u64) -> bool {
        self.replays.get(&token).is_some()
    }

    pub fn get_replay(&self, token: u64) -> &ReplayState {
        self.replays.get(&token).unwrap()
    }

    pub fn get_mut_replay(&mut self, token: u64) -> &mut ReplayState {
        self.replays.get_mut(&token).unwrap()
    }

    pub fn discard_replay(&mut self, token: u64) {
        self.replays.remove(&token).unwrap().discard();
    }
}

// INVARIANTS:
// Streams in quorum have at least as much data as canonical stream and agree with it.
// Streams in reserve either have more data than canonical stream or are not finished.
// All other streams are discarded.
struct MergeQuorumState {
    pub s: SharedState,
    quorum: HashSet<u64>,
    reserve: HashSet<u64>,
    quorum_diverges_at: Option<usize>,
}

// INVARIANTS:
// Candidates have more data than canonical stream and agree with it.
// Streams in reserve either have more data than canonical stream or are not finished.
// All other streams are discarded.
struct MergeStalemateState {
    pub s: SharedState,
    candidates: HashMap<u8, Vec<u64>>,
    reserve: HashSet<u64>,
}

impl MergeQuorumState {
    pub fn from_stalemate(shared: SharedState, mut good_replays: Vec<u64>, mut reserve: HashSet<u64>) -> Self {
        // All good replays agree on the first byte. Pick a new quorum.
        debug_assert!(!good_replays.is_empty());

        // Take longest replays
        good_replays.sort_unstable_by_key(|&id| shared.get_replay(id).data_len());
        good_replays.reverse();

        // Collect enough replays for quorum, drop the rest
        let new_quorum_len = std::cmp::min(good_replays.len(), shared.target_quorum_size);
        let new_quorum: HashSet<u64> = good_replays.drain(..new_quorum_len).collect();
        reserve.union(&good_replays.drain(..).collect());

        MergeQuorumState {
            s: shared,
            quorum: new_quorum,
            reserve,
            quorum_diverges_at: None,
        }

        /* TODO merge data */
    }

    pub fn has_to_enter_stalemate(&self) -> bool {
        // Did we send all merged data?
        if self.s.data_merged > self.s.data_to_send {
            false
        } else {
            !self.can_merge_more_data()
        }
    }

    pub fn enter_stalemate(self) -> MergeStalemateState {
        let reserve = self.reserve;
        reserve.union(&self.quorum);
        MergeStalemateState::from_quorum(self.s, reserve)
    }

    pub fn can_merge_more_data(&self) -> bool {
        if let Some(..) = self.quorum_diverges_at {
            return false
        }
        // Do all quorum streams have some more data?
        self.quorum.iter().map(|id| self.s.get_replay(*id).data_len()).min().unwrap() > self.s.data_merged
    }

    pub fn merge_more_data(&mut self) {
        debug_assert!(self.can_merge_more_data());

        let shortest_id = *self.quorum.iter().min_by_key(|id| self.s.get_replay(**id).data_len()).unwrap();
        let shortest = self.s.get_replay(shortest_id);

        let cmp_start = std::cmp::max(shortest.data_len() - self.s.stream_cmp_distance, self.s.data_merged);
        let mut all_common_prefix = shortest.data_len();

        for id in self.quorum.iter() {
            if *id == shortest_id {
                continue;
            }
            let pfx = shortest.common_prefix_from(self.s.get_replay(*id), cmp_start);
            all_common_prefix = std::cmp::min(all_common_prefix, pfx);
        }

        if all_common_prefix < shortest.data_len() {
            self.quorum_diverges_at = Some(all_common_prefix);
        }
        self.s.data_merged = all_common_prefix;
    }

    fn add_replay(&mut self, r: WReplayRef) -> u64 {
        let token = self.s.add_replay(r);
        self.reserve.insert(token);
        token
    }

    fn replay_new_data(&mut self, _id: u64) {
        // pass
    }

    fn replay_new_delayed_data(&mut self, id: u64, data_len: usize) {
        if self.quorum.contains(&id) {
            self.s.data_to_send = std::cmp::min(data_len, self.s.data_merged);
        }
    }

    fn end_replay(&mut self, id: u64) {
        if self.reserve.contains(&id) {
            if self.s.get_replay(id).data_len() <= self.s.data_merged {
                self.reserve.remove(&id);
            }
        }
    }
}

impl MergeStalemateState {
    pub fn new() -> Self {
        Self {
            s: SharedState::new(),
            candidates: HashMap::new(),
            reserve: HashSet::new(),
        }
    }

    pub fn from_quorum(shared: SharedState, reserve: HashSet<u64>) -> Self {
        let mut me = Self {
            s: shared,
            candidates: HashMap::new(),
            reserve: reserve.clone(),
        };
        for id in reserve {
            me.try_move_replay_to_candidates(id);
        }
        me
    }

    pub fn can_exit_stalemate(&self) -> bool {
        if self.candidates.is_empty() {
            false
        } else if self.reserve.is_empty() {
            true
        } else {
            self.candidates.values().map(|c| c.len()).max().unwrap() >= self.s.target_quorum_size
        }
    }

    pub fn exit_stalemate(mut self) -> MergeQuorumState {
        debug_assert!(!self.candidates.is_empty());

        // Sort by stream count, then by longest stream.
        let replay_len  = |&id| self.s.get_replay(id).data_len();
        let best_byte = *self.candidates.iter().map(|(k, v)| {
            (v.len(), v.iter().map(replay_len).max().unwrap(), k)
        }).max().unwrap().2;

        let good_replays = self.candidates.remove(&best_byte).unwrap();
        // Discard all the rest, they diverge
        for (_, v) in self.candidates.iter() {
            for id in v.iter() {
                self.s.discard_replay(*id);
            }
        }
        MergeQuorumState::from_stalemate(self.s, good_replays, self.reserve)
    }

    fn try_move_replay_to_candidates(&mut self, id: u64) {
        let replay = self.s.get_replay(id).replay.borrow();
        if replay.get_data().len() <= self.s.data_merged {
            debug_assert!(!replay.is_finished());
            return;
        } else {
            self.reserve.remove(&id);
            let next_byte = replay.get_data().at(self.s.data_merged);
            if self.candidates.get(&next_byte).is_none() {
                self.candidates.insert(next_byte, Vec::new());
            }
            let cands_for_byte = self.candidates.get_mut(&next_byte).unwrap();
            cands_for_byte.push(id);
        }
    }

    fn add_replay(&mut self, r: WReplayRef) -> u64 {
        let token = self.s.add_replay(r);
        self.reserve.insert(token);
        token
    }

    fn replay_new_data(&mut self, id: u64) {
        if self.reserve.contains(&id) {
            self.try_move_replay_to_candidates(id);
        }
    }

    fn replay_new_delayed_data(&mut self, _id: u64, _data_len: usize) {
        // pass
    }

    fn end_replay(&mut self, id: u64) {
        if self.reserve.contains(&id) {
            if self.s.get_replay(id).data_len() <= self.s.data_merged {
                self.reserve.remove(&id);
            }
        }
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

    fn work_state_until_stable(&mut self) {
        while (self.should_change_status()) {
        }
    }
}

impl MergeStrategy for QuorumMergeStrategy {
    fn replay_added(&mut self, r: WReplayRef) -> u64 {
        both!(self, s => s.add_replay(r))
    }

    fn replay_removed(&mut self, id: u64) {
        both!(self, s => s.end_replay(id))
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
        both!(self, s => s.replay_new_data(id));
        self.work_state_until_stable();
    }

    fn replay_new_delayed_data(&mut self, id: u64, data_len: usize) {
        both!(self, s => s.replay_new_delayed_data(id, data_len));
        self.work_state_until_stable();
    }

    fn get_merged_replay(&self) -> MReplayRef {
        both!(self, s => s.s.canonical_stream.clone())
    }
}
