use std::{cell::RefCell, collections::HashMap, collections::HashSet, rc::Rc};

use crate::{
    replay::streams::MReplayRef, replay::streams::MergedReplay, replay::streams::WReplayRef,
    util::buf_traits::DiscontiguousBuf, util::buf_traits::DiscontiguousBufExt,
};

use super::merge_strategy::MergeStrategy;

// This merge strategy tries to merge replays in such a way that at least N replays agree on the
// merged data. To do that, it selects a subset of N replays called a quorum and compares their
// data, which is then added to the merged stream. It tries to process replays in quorum in large
// chunks and makes some shortcuts to avoid comparing a lot of data.
//
// Detailed explanation below.
//
// We define:
// * R and C as defined in merge_strategy.rs, ignoring headers.
// * Parameters:
//   * stream_cmp_distance, in bytes,
//   * target_quorum_size, in count.

// For a replay r in R, we define:
// * r matches C iff C's data is a (non-strict) prefix of r's data.
// * r in R diverges from C iff either:
//   * Common prefix of r and C is not equal to either r or C,
//   * r has less data than C and r is finished.
//
// Intuitively:
//  * r matching C means that its data "agrees" with all canonical data.
//  * r diverging from C means that either r's data doesn't match C's at some point or r ended
//    early, so we can't use it for further merging.
// * If neither is true, then r has the same data as C, but has less of it and is not finished.
//
// In order to avoid matching a lot of data, we make a following "shortcut" assumption:
// * As long as, every time we check, r and C's data is equal at a suffix of stream_cmp_distance
//   bytes of C, r does not diverge from C.
// In other words, whenever we check if r diverges from C, we can check just the last
// stream_cmp_distance bytes.
//
// We keep the state of a replay r in R in the struct below:

struct ReplayState {
    replay: WReplayRef,         // Writer replay, updated from connection in another task.
    canon_replay: MReplayRef,   // Canonical replay C.
    stream_cmp_distance: usize, // As defined above.

    // Fields used to lazily check relation of r towards C.
    data_matching_canon: usize,
    diverges: bool,
}

// And we check its diverge status (along with a helper functions for comparing streams and other
// stuff) here. Proving correctness is not hard and left to the reader.

// TODO test this in isolation.
impl ReplayState {
    fn new(replay: WReplayRef, canon_replay: MReplayRef, stream_cmp_distance: usize) -> Self {
        Self {
            replay,
            canon_replay,
            stream_cmp_distance,
            data_matching_canon: 0,
            diverges: false,
        }
    }
    fn data_len(&self) -> usize {
        self.replay.borrow().get_data().len()
    }

    fn canon_len(&self) -> usize {
        self.canon_replay.borrow().get_data().len()
    }

    fn common_len(&self) -> usize {
        std::cmp::min(self.canon_len(), self.data_len())
    }

    fn delayed_data_len(&self) -> usize {
        self.replay.borrow().get_delayed_data_len()
    }

    fn is_finished(&self) -> bool {
        self.replay.borrow().is_finished()
    }

    fn common_prefix_from(&self, other: &ReplayState, start: usize) -> usize {
        self.replay
            .borrow()
            .get_data()
            .common_prefix_from(other.replay.borrow().get_data(), start)
    }

    fn diverges_from_canon(&mut self) -> bool {
        self.match_with_canon_stream();
        self.diverges
    }

    fn canon_match_start(&self) -> usize {
        debug_assert!(self.data_matching_canon <= self.common_len());
        let optimized_match_start = self.common_len().saturating_sub(self.stream_cmp_distance);
        std::cmp::max(self.data_matching_canon, optimized_match_start)
    }

    fn set_diverged(&mut self) {
        self.replay.borrow_mut().discard_all();
        self.diverges = true;
    }

    fn match_with_canon_stream(&mut self) {
        if self.diverges {
            return;
        }
        if self.data_len() < self.canon_len() && self.is_finished() {
            self.set_diverged();
            return;
        }
        if self.data_matching_canon == self.common_len() {
            return;
        }

        let match_start = self.canon_match_start(); // this borrows
        self.data_matching_canon = self
            .replay
            .borrow()
            .get_data()
            .common_prefix_from(self.canon_replay.borrow().get_data(), match_start);

        if self.data_matching_canon != self.common_len() {
            self.set_diverged();
        }

        self.discard_unneeded_data();
    }

    // To save memory, we discard data before canon_match_start(), or all data if we diverged.
    // It is an error to access replay data from before canonical replay's length, and it's an
    // error to access any data of a diverged replay.
    fn discard_unneeded_data(&mut self) {
        // If we diverged, we already discarded everything
        if !self.diverges {
            let match_start = self.canon_match_start(); // this borrows
            self.replay.borrow_mut().discard(match_start);
        }
    }

    // For when we *know* the replay matches.
    fn explicitly_set_matching(&mut self) {
        debug_assert!(!self.diverges);
        debug_assert!(self.data_len() >= self.canon_len());
        self.data_matching_canon = self.canon_len();
        self.discard_unneeded_data();
    }

    // For when we *know* the replay does not match.
    fn explicitly_set_diverged(&mut self) {
        self.set_diverged();
    }
}

// Now, the actual merge strategy.
//

struct SharedState {
    token: u64,
    stream_cmp_distance: usize,
    delayed_data_started: bool,
    target_quorum_size: usize,
    replays: HashMap<u64, ReplayState>,
    canonical_stream: MReplayRef,
}

impl SharedState {
    fn new(target_quorum_size: usize, stream_cmp_distance: usize) -> Self {
        Self {
            token: 0,
            stream_cmp_distance,
            delayed_data_started: false,
            target_quorum_size,
            replays: HashMap::new(),
            canonical_stream: Rc::new(RefCell::new(MergedReplay::new())),
        }
    }

    fn add_replay(&mut self, r: WReplayRef) -> u64 {
        let token = self.token;
        let replay = ReplayState::new(r, self.canonical_stream.clone(), self.stream_cmp_distance);
        self.replays.insert(token, replay);
        self.token += 1;
        token
    }

    fn get_replay(&self, token: u64) -> &ReplayState {
        self.replays.get(&token).unwrap()
    }

    fn get_mut_replay(&mut self, token: u64) -> &mut ReplayState {
        self.replays.get_mut(&token).unwrap()
    }

    fn merged_data_len(&self) -> usize {
        self.canonical_stream.borrow().get_data().len()
    }

    fn merged_delayed_data_len(&self) -> usize {
        self.canonical_stream.borrow().delayed_data_len()
    }

    fn append_canon_data(&mut self, id: u64, to: usize) {
        {
            let mut canon_replay = self.canonical_stream.borrow_mut();
            let source_replay = self.get_replay(id).replay.borrow();
            canon_replay.add_data(&*source_replay, to);
        }
        for r in self.replays.values_mut() {
            r.discard_unneeded_data();
        }
    }

    fn update_merged_delayed_data_len(&mut self, mut hint: usize) {
        hint = std::cmp::min(hint, self.merged_data_len());
        if hint <= self.merged_delayed_data_len() {
            return;
        }
        self.canonical_stream.borrow_mut().advance_delayed_data(hint);
    }
}

// The strategy switches between two states - a quorum and a stalemate.
// During a quorum, it chooses a subset of replays and merges data from them into C as far as it
// can. Once merge point is reached, it transitions to a stalemate.
// During a stalemate, it tries to collect a subset of replays that agree with C and agree on C's next
// byte. Once it does, it transitions to quorum with that subset.
//
// In detail:
//
// STALEMATE:
// Find a subset of replays that matches C and agrees on the next byte. Once found, advance the
// stream by that byte and transition to quorum.

pub struct MergeStalemateState {
    s: SharedState,
    candidates: HashMap<u8, Vec<u64>>,
    reserve: HashSet<u64>,
}

// * Stalemate has a set Res and a map Cand.
// * At any time between callbacks (ignoring changes we weren't yet notified of):
//   * Replays in Cand are exactly those that match C and are longer than C.
//   * Replays in Res are no longer than C and are not finished.
//   * All replays in neither either diverge from C, or are no longer than C and finished.
//   * Notice that every replay satisfies at least one of the above.
// * A stalemate state is constructed with a set of replays I, which is a subset of R.
//   * Every replay outside I diverges from C.
//   * Replays from I are distributed between Cand and Res according to above rules.
// * Whether a stalemate can be resolved is decided as follows:
//   * Stalemate stays unresolved as long as no delayed positions have ever been set. (This is a
//     hack to prevent too eager stalemate resolution at the start, when replays are still being
//     added.)
//   * If one of entries in Cand has at least target_quorum_size entries, the stalemate can be
//     resolved.
//   * Otherwise stalemate can resolved if set Res is empty and map Cand is not empty.
// * Once a stalemate is resolved, it is turned into a quorum.
//   * Replays from the most numerous entry in Cand become a "good replay" set G.
//   * C is advanced by one byte that equals the extra byte replays in G agree on.
//   * All other entries in Cand are discarded.
//   * G and Res are given to quorum constructor.
//   * Notice that all replays in G agree with C and all replays outside G and Res diverge from C.

impl MergeStalemateState {
    fn new(target_quorum_size: usize, stream_cmp_distance: usize) -> Self {
        Self {
            s: SharedState::new(target_quorum_size, stream_cmp_distance),
            candidates: HashMap::new(),
            reserve: HashSet::new(),
        }
    }

    fn from_quorum(shared: SharedState, reserve: HashSet<u64>) -> Self {
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

    fn try_move_replay_to_candidates(&mut self, id: u64) {
        {
            let replay = self.s.get_replay(id).replay.borrow();
            if replay.get_data().len() <= self.s.merged_data_len() {
                if replay.is_finished() {
                    self.reserve.remove(&id); // Replay finished short.
                }
                return; // Replay is short, still in reserve.
            }
        }
        {
            self.reserve.remove(&id);
            if self.s.get_mut_replay(id).diverges_from_canon() {
                return; // Replay diverged in the meantime.
            }
        }
        {
            let replay = self.s.get_replay(id).replay.borrow(); // Replay matches and can become a candidate.
            let next_byte = replay.get_data().at(self.s.merged_data_len());
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

    fn replay_data_updated(&mut self, id: u64) {
        // Replay may have advanced from a stalemate.
        if self.reserve.contains(&id) {
            self.try_move_replay_to_candidates(id);
        }

        // Delayed data check.
        if self.s.delayed_data_started || self.s.get_replay(id).delayed_data_len() == 0 {
            return;
        }
        self.s.delayed_data_started = true;
    }

    fn replay_ended(&mut self, id: u64) {
        // Replay ended while short.
        if self.reserve.contains(&id) {
            self.try_move_replay_to_candidates(id);
        }
    }

    fn can_exit_stalemate(&self) -> bool {
        if !self.s.delayed_data_started {
            // Wait as long as we can at the start
            return false;
        }
        if self.candidates.is_empty() {
            // We have no candidates yet
            return false;
        }
        if self.reserve.is_empty() {
            // We ran out of reserve replays, pick *something*
            return true;
        }
        // Is any candidate set big enough?
        self.candidates.values().map(|c| c.len()).max().unwrap() >= self.s.target_quorum_size
    }

    fn exit_stalemate(mut self) -> MergeQuorumState {
        debug_assert!(!self.candidates.is_empty());

        // Sort by stream count, then by longest stream.
        let replay_len = |&id| self.s.get_replay(id).data_len();
        let best_byte = *self
            .candidates
            .iter()
            .map(|(k, v)| (v.len(), v.iter().map(replay_len).max().unwrap(), k))
            .max()
            .unwrap()
            .2;

        let good_replays = self.candidates.remove(&best_byte).unwrap();

        // Advance replay by 1 byte
        let good_replay = *good_replays.get(0).unwrap();
        let byte_pos = self.s.merged_data_len();
        self.s.append_canon_data(good_replay, byte_pos + 1);
        for id in good_replays.iter() {
            self.s.get_mut_replay(*id).explicitly_set_matching();
        }

        // Discard all the rest, they diverge
        for (_, v) in self.candidates.iter() {
            for id in v.iter() {
                self.s.get_mut_replay(*id).explicitly_set_diverged();
            }
        }
        MergeQuorumState::from_stalemate(self.s, good_replays, self.reserve)
    }
}

// QUORUM:
// Have a set of replays Q that matches C. When constructed, merge replays in quorum and add the
// common prefix to C. Update C's delayed position as a minimum of Q's replays' delayed positions.
// Once C's delayed position reaches end of merged data, enter a stalemate.

pub struct MergeQuorumState {
    s: SharedState,
    quorum: HashSet<u64>,
    reserve: HashSet<u64>,
}

// * A quorum has a set Q and a set Res.
// * At any time between callbacks (ignoring changes we weren't yet notified of):
//   * Q has at most target_quorum_size members.
//   * All replays in Q match C.
//   * All replays outside Q and Res are diverged.
//   * C's delayed position is equal to a minimum of:
//     * A minimum of delayed positions of replays in Q,
//     * Its own data length.
// * A quorum is constructed with a (non-empty) set of "good replays" G that match C and a reserve
//   set Res. Q is populated with replays from G. Remaining replays in G are added into Res.
// * When constructed, quorum performs a "merging step". In a merging step, replays in Q are
//   compared from the end of C and their common prefix is appended to C.
// * The quorum transitions to a stalemate when C's delayed position reaches its data length.
//
// * Transitioning to a stalemate happens as follows:
//   * Replays from Q are added to Res.
//   * Stalemate is constructed with Res.
//   * Notice that all replays outside Res are diverged.

impl MergeQuorumState {
    fn from_stalemate(shared: SharedState, mut good_replays: Vec<u64>, mut reserve: HashSet<u64>) -> Self {
        debug_assert!(!good_replays.is_empty());

        // Take longest replays
        good_replays.sort_unstable_by_key(|&id| shared.get_replay(id).data_len());
        good_replays.reverse();

        // Collect enough replays for quorum, move rest to reserve
        let new_quorum_len = std::cmp::min(good_replays.len(), shared.target_quorum_size);
        let new_quorum: HashSet<u64> = good_replays.drain(..new_quorum_len).collect();
        reserve.extend(good_replays.drain(..));

        let mut me = MergeQuorumState {
            s: shared,
            quorum: new_quorum,
            reserve,
        };
        me.merge_more_data();
        me
    }

    fn add_replay(&mut self, r: WReplayRef) -> u64 {
        let token = self.s.add_replay(r);
        self.reserve.insert(token);
        token
    }

    // We chose minimum rather than maximum due to delayed position behaviour. When a replay ends,
    // its delayed position immediately jumps to end of data, removing the 5 minute delay, so we
    // can deliver a finished replay immediately. If we used max, this could cause rapid switches
    // between quorum and stalemate: for a finished / unfinished replay pair, stalemate would
    // immediately return it while quorum would merge a bit of data, then transition to stalemate
    // as the finished replay's delayed position points to the end of data. With a minimum, we'd
    // only move the delayed position to the end once all quorum replays finish.
    // Using a minimum here is fine. Delayed data always catches up with normal data, even if a
    // replay stalls, so we'll always eventually switch to a stalemate.
    fn quorum_delayed_position(&self) -> usize {
        self.quorum
            .iter()
            .map(|id| self.s.get_replay(*id).delayed_data_len())
            .min()
            .unwrap()
    }

    fn replay_data_updated(&mut self, id: u64) {
        if self.quorum.contains(&id) {
            self.update_delayed_position();
        }
    }

    fn replay_ended(&mut self, _id: u64) {
        // pass, we don't care
    }

    fn has_to_enter_stalemate(&self) -> bool {
        self.s.merged_delayed_data_len() >= self.s.merged_data_len()
    }

    fn enter_stalemate(self) -> MergeStalemateState {
        let s = self.s;
        let mut reserve = self.reserve;
        reserve.extend(self.quorum.iter());
        MergeStalemateState::from_quorum(s, reserve)
    }

    fn calculate_quorum_prefix(&mut self) -> usize {
        debug_assert!(!self.quorum.is_empty());

        let shortest_id = *self
            .quorum
            .iter()
            .min_by_key(|id| self.s.get_replay(**id).data_len())
            .unwrap();
        let shortest = self.s.get_replay(shortest_id);

        let cmp_start = self.s.merged_data_len();
        let mut common_prefix = shortest.data_len();

        for id in self.quorum.iter() {
            if *id == shortest_id {
                continue;
            }
            let pfx = shortest.common_prefix_from(self.s.get_replay(*id), cmp_start);
            common_prefix = std::cmp::min(common_prefix, pfx);
        }
        common_prefix
    }

    fn merge_more_data(&mut self) {
        let common_prefix = self.calculate_quorum_prefix();
        let any_in_quorum = *self.quorum.iter().next().unwrap();
        self.s.append_canon_data(any_in_quorum, common_prefix);
        for id in self.quorum.iter() {
            self.s.get_mut_replay(*id).explicitly_set_matching();
        }
        self.update_delayed_position();
    }

    fn update_delayed_position(&mut self) {
        self.s.update_merged_delayed_data_len(self.quorum_delayed_position());
    }
}

// Now the merge strategy interface.

pub enum QuorumMergeStrategy {
    Quorum(MergeQuorumState),
    Stalemate(MergeStalemateState),
    Swapping, // Dummy value so we can swap between quorum and stalemate. Never used otherwise.
}

macro_rules! both {
    ($value:expr, $pattern:pat => $result:expr) => {
        match $value {
            Self::Quorum($pattern) => $result,
            Self::Stalemate($pattern) => $result,
            Self::Swapping => panic!("Programmer error - we're swapping state right now!"),
        }
    };
}

impl QuorumMergeStrategy {
    pub fn new(target_quorum_size: usize, stream_cmp_distance: usize) -> Self {
        Self::Stalemate(MergeStalemateState::new(target_quorum_size, stream_cmp_distance))
    }

    fn should_change_state(&self) -> bool {
        match &self {
            Self::Quorum(s) => s.has_to_enter_stalemate(),
            Self::Stalemate(s) => s.can_exit_stalemate(),
            Self::Swapping => panic!("Programmer error - we're swapping state right now!"),
        }
    }

    fn work_state_until_stable(&mut self) {
        while self.should_change_state() {
            let mut tmp = Self::Swapping;
            std::mem::swap(&mut tmp, self);
            *self = match tmp {
                Self::Quorum(s) => Self::Stalemate(s.enter_stalemate()),
                Self::Stalemate(s) => Self::Quorum(s.exit_stalemate()),
                Self::Swapping => panic!("Programmer error - we're swapping state right now!"),
            }
        }
    }
}

impl MergeStrategy for QuorumMergeStrategy {
    fn replay_added(&mut self, r: WReplayRef) -> u64 {
        let token = both!(self, s => s.add_replay(r));
        self.work_state_until_stable();
        token
    }

    fn replay_removed(&mut self, id: u64) {
        both!(self, s => s.replay_ended(id));
        self.work_state_until_stable();
    }

    fn replay_header_added(&mut self, id: u64) {
        // Accept the first header we get.
        // As a bonus, this guarantees that canonical stream will be in data stage once we start
        // merging data.
        let replay = both!(self, s => s.s.get_replay(id));
        let header = replay.replay.borrow_mut().take_header();
        let mut canonical_stream = both!(self, s => s.s.canonical_stream.borrow_mut());
        if canonical_stream.get_header().is_none() {
            canonical_stream.add_header(header);
        }
    }

    fn replay_data_updated(&mut self, id: u64) {
        both!(self, s => s.replay_data_updated(id));
        self.work_state_until_stable();
    }

    fn get_merged_replay(&self) -> MReplayRef {
        both!(self, s => s.s.canonical_stream.clone())
    }

    fn finish(&mut self) {
        // We know that delayed position for all replays is at the end of their data.
        // If we were in a quorum state, then delayed position of merged replay would equal its
        // data length (as its data can't be longer than minimum data length in quorum, which equals
        // minimum delayed data length in quorum).
        // Then, should_change_state() would return true, but we always end calls with it being
        // false. Therefore, we're in a stalemate.
        match self {
            Self::Swapping => panic!("Programmer error - we're swapping state right now!"),
            Self::Quorum(..) => panic!("Expected to finish merge strategy in a stalemate"),
            Self::Stalemate(s) => {
                // Not in a quorum. Delayed replay position must be equal to its data len.
                let data_len = s.s.merged_data_len();
                let position = s.s.merged_delayed_data_len();
                debug_assert_eq!(data_len, position);
                // All replays are finished, so Res is empty.
                debug_assert!(s.reserve.is_empty());
                // should_change_state() returns false, so stalemate cannot be resolved.
                // * If delayed positions have been set at least once, then there are no replays in
                //   Cand, otherwise stalemate would resolve.
                // * Otherwise, no data positions were ever set either. Therefore all replays
                //   finished empty, so nothing was ever added to Cand.
                // Therefore Cand is empty.
                debug_assert!(s.candidates.is_empty());
            }
        }
        self.get_merged_replay().borrow_mut().finish();
    }
}

// Now, a justification why it works.
// Of course, we claim that invariants specified for quorum and stalemate states (both
// initialization and between-calls invariants) are satisfied. That's something for tests to
// reassure us of and reader to verify.
//
// Our first claim is that we don't loop infinitely in work_state_until stable. That's easy -
// resolving a stalemate always advances data by one byte, and we can't advance beyond data length
// of the longest replay.
//
// Our second claim is that once we're finished, the canonical replay C is equal to one of sent
// replays. We start with assertions in the finish() method above. We skip the case with no replays
// as trivial.
// The final stalemate has empty Cand. According to invariants, it means there are no replays that
// match C and are longer than C. However, C is always a prefix of at least one replay. That replay
// matches C by definition, therefore it's not longer than C, therefore it's equal to C. QED
//
// Our third claim broadly says that C will never equal a replay that split off from others alone
// with its own data. That's pretty easy.
// We add data to C either in quorum, comparing entire quorum, or in stalemate, comparing best
// fit.
// * For a stalemate, we'd have to pick the lone replay in stalemate resolution. That won't
//   happen since the rest of replays will eventually be moved from the reserve set to the candidate
//   map and form a better candidate group.
// * For a quorum, the lone replay cannot be a quorum all alone, because of the point above. If
//   it's with other replays, then merging will stop before the lone replay starts to differ, so
//   lone replay's data won't be merged here either.
//
// Our fourth claim talks about performance. Worst case scenario is quorum never advancing its data
// and stalemate having to work for every single byte. Why does it not happen?
// * We can assume quorum ends due to diverging data very few times, since every time it does, we
//   discard at least one replay.
// * Otherwise, the quorum ends because the delayed position caught up with the data we merged. If
//   that happened after K < N minutes, then the shortest replay has been stalling for N - K
//   minutes at that position. Total replay stalling time can't exceed (replay count) minutes per
//   minute, so we won't enter stalemate more often than (replay count + 1) times per N minutes,
//   amortized.
//
// Fifth claim is memory usage. We always discard any data that's stream_cmp_distance behind canon.
// Once in a quorum, we won't merge more data until delayed data catches up, so we keep at most
// last N minutes of data in each replay. It's fine, potentially could be improved if we merge more
// eagerly?
// In a stalemate, there could be pathological conditions where a replay stalls data (see below),
// stopping stalemate resolution for the rest of the replay IF there is otherwise no quorum. That
// would accumulate data until end of the replay. This should happen very rarely, and maybe there
// are ways to mitigate that? See below.
//
// Sixth claim is (no) resilience to replays that stop sending data, but don't *finish* because TCP
// connection is kept open. We can't time out replays quickly, because long pauses in the game
// (e.g. connectivity issues) happen sometimes. Replays are timed out *eventually*, so everything
// *eventually* works out, but if we wait for such a bad replay, a replay watcher could get
// confused and say the server is broken. It kind of would be.
// In the quorum, a misbehaving replay will block merging, but won't block reaching the merge point
// and entering stalemate. In a stalemate, things are fine as long as we find a quorum without
// misbehaving replays, which is not the case in 1v1 and when almost all players left.
//
// Note that it doesn't seem to be a problem in the current python server which uses the same merge
// strategy, so this might not be a problem in practice.
//

// TODO - parametrize with stream cutoff and quorum once we make them configurable.
#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::QuorumMergeStrategy;
    use crate::util::{buf_traits::ReadAtExt, test::setup_logging};
    use crate::{
        replay::receive::merge_strategy::MergeStrategy, replay::streams::ReplayHeader, replay::streams::WriterReplay,
        util::buf_traits::DiscontiguousBuf,
    };
    use std::{cell::RefCell, io::Read, rc::Rc};

    fn strat() -> QuorumMergeStrategy {
        QuorumMergeStrategy::new(2, 4096)
    }

    #[test]
    fn test_strategy_ends_stream_when_finalized() {
        let mut strat = strat();
        let stream1 = Rc::new(RefCell::new(WriterReplay::new()));
        let token1 = strat.replay_added(stream1.clone());
        stream1.borrow_mut().finish();
        strat.replay_removed(token1);
        strat.finish();

        let out_stream_ref = strat.get_merged_replay();
        let out_stream = out_stream_ref.borrow();
        assert!(out_stream.get_header().is_none());
        assert_eq!(out_stream.get_data().len(), 0);
    }

    #[test]
    fn test_strategy_picks_at_least_one_header() {
        let mut strat = strat();
        let stream1 = Rc::new(RefCell::new(WriterReplay::new()));
        let stream2 = Rc::new(RefCell::new(WriterReplay::new()));
        stream2.borrow_mut().add_header(ReplayHeader { data: vec![1, 3, 3, 7] });

        let token1 = strat.replay_added(stream1.clone());
        let token2 = strat.replay_added(stream2.clone());
        strat.replay_header_added(token2);

        stream1.borrow_mut().finish();
        stream2.borrow_mut().finish();
        strat.replay_removed(token1);
        strat.replay_removed(token2);
        strat.finish();

        let out_stream_ref = strat.get_merged_replay();
        let out_stream = out_stream_ref.borrow();
        assert!(out_stream.get_header().unwrap().data == vec!(1, 3, 3, 7));
    }

    #[test]
    fn test_strategy_gets_all_data_of_one() {
        let mut strat = strat();
        let stream1 = Rc::new(RefCell::new(WriterReplay::new()));
        stream1.borrow_mut().add_header(ReplayHeader { data: vec![1, 3, 3, 7] });

        let token1 = strat.replay_added(stream1.clone());
        strat.replay_header_added(token1);

        stream1.borrow_mut().add_data(&[1, 2, 3, 4]);
        strat.replay_data_updated(token1);
        stream1.borrow_mut().set_delayed_data_len(4);
        strat.replay_data_updated(token1);
        stream1.borrow_mut().add_data(&[5, 6]);
        stream1.borrow_mut().set_delayed_data_len(5);
        strat.replay_data_updated(token1);
        stream1.borrow_mut().add_data(&[7, 8]);
        stream1.borrow_mut().set_delayed_data_len(8);
        strat.replay_data_updated(token1);

        stream1.borrow_mut().finish();
        strat.replay_removed(token1);
        strat.finish();

        let out_stream_ref = strat.get_merged_replay();
        let out_stream = out_stream_ref.borrow();
        let out_data = out_stream.get_data();

        assert!(out_data.len() == 8);
        let out_buf: &mut [u8] = &mut [0; 8];
        out_data.reader().read(out_buf).unwrap();
        assert_eq!(out_buf, &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn test_strategy_gets_common_prefix_of_all() {
        let mut strat = strat();
        let stream1 = Rc::new(RefCell::new(WriterReplay::new()));
        let stream2 = Rc::new(RefCell::new(WriterReplay::new()));

        stream1.borrow_mut().add_header(ReplayHeader { data: vec![1, 3, 3, 7] });
        stream2.borrow_mut().add_header(ReplayHeader { data: vec![1, 3, 3, 7] });
        let token1 = strat.replay_added(stream1.clone());
        let token2 = strat.replay_added(stream2.clone());
        strat.replay_header_added(token1);
        strat.replay_header_added(token2);

        stream1.borrow_mut().add_data(&[1, 2, 3, 4]);
        stream1.borrow_mut().set_delayed_data_len(4);
        strat.replay_data_updated(token1);

        stream2.borrow_mut().add_data(&[1, 2]);
        stream2.borrow_mut().set_delayed_data_len(2);
        strat.replay_data_updated(token2);

        stream1.borrow_mut().add_data(&[5, 6]);
        stream1.borrow_mut().set_delayed_data_len(6);
        strat.replay_data_updated(token1);

        stream2.borrow_mut().add_data(&[3, 20, 21, 22, 23]);
        stream2.borrow_mut().set_delayed_data_len(7);
        strat.replay_data_updated(token2);

        stream1.borrow_mut().add_data(&[7, 8]);
        stream1.borrow_mut().set_delayed_data_len(8);
        strat.replay_data_updated(token1);

        stream1.borrow_mut().finish();
        stream2.borrow_mut().finish();
        strat.replay_removed(token1);
        strat.replay_removed(token2);
        strat.finish();

        let out_stream_ref = strat.get_merged_replay();
        let out_stream = out_stream_ref.borrow();
        let out_data = out_stream.get_data();

        assert!(out_data.len() == 7 || out_data.len() == 8);
        let out_buf: &mut [u8] = &mut [0; 8];
        out_data.reader().read(out_buf).unwrap();
        assert_eq!(out_buf[..3], [1, 2, 3]);
        if out_data.len() == 7 {
            assert_eq!(out_buf[3..7], [20, 21, 22, 23]);
        } else {
            assert_eq!(out_buf[3..8], [4, 5, 6, 7, 8]);
        }
    }

    #[test]
    fn test_strategy_later_has_more_data() {
        let mut strat = strat();
        let stream1 = Rc::new(RefCell::new(WriterReplay::new()));
        let stream2 = Rc::new(RefCell::new(WriterReplay::new()));

        stream1.borrow_mut().add_header(ReplayHeader { data: vec![1, 3, 3, 7] });
        stream2.borrow_mut().add_header(ReplayHeader { data: vec![1, 3, 3, 7] });
        let token1 = strat.replay_added(stream1.clone());
        let token2 = strat.replay_added(stream2.clone());
        strat.replay_header_added(token1);
        strat.replay_header_added(token2);

        stream1.borrow_mut().add_data(&[1, 2, 3, 4]);
        stream1.borrow_mut().set_delayed_data_len(4);
        strat.replay_data_updated(token1);

        stream2.borrow_mut().add_data(&[1, 2, 3, 4, 5, 6]);
        stream2.borrow_mut().set_delayed_data_len(6);
        strat.replay_data_updated(token2);

        stream1.borrow_mut().finish();
        stream2.borrow_mut().finish();
        strat.replay_removed(token1);
        strat.replay_removed(token2);
        strat.finish();

        let out_stream_ref = strat.get_merged_replay();
        let out_stream = out_stream_ref.borrow();
        let out_data = out_stream.get_data();

        assert!(out_data.len() == 6);
        let out_buf: &mut [u8] = &mut [0; 6];
        out_data.reader().read(out_buf).unwrap();
        assert_eq!(out_buf, &[1, 2, 3, 4, 5, 6]);
    }

    // FIXME tweak so we can test small comparison cutoffs.
    fn simple_fuzzing_round() {
        let mut rng = rand::thread_rng();
        let mut strat = QuorumMergeStrategy::new(2, 512);
        let count = 8;
        let chunk = 4;
        let replay_len = 400;
        let data_error_chance = 200;
        let early_exit_chance = 200;

        let mut streams = Vec::new();
        let mut final_datas = Vec::new();

        // Add all streams.
        for _ in 0..count {
            let stream = Rc::new(RefCell::new(WriterReplay::new()));
            stream.borrow_mut().add_header(ReplayHeader { data: vec![1, 3, 3, 7] });
            let token = strat.replay_added(stream.clone());
            strat.replay_header_added(token);
            let data: Vec<u8> = Vec::new();
            streams.push((stream, token, data));
        }

        loop {
            // Pick a non-finished stream to randomly advance.
            if final_datas.len() == count {
                break;
            }
            let idx = rng.gen_range(0..count);
            let (s, token, data) = streams.get_mut(idx).unwrap();
            if s.borrow().is_finished() {
                continue;
            }

            let mut modified = false;
            if rand::random() {
                // Randomly advance data.
                let mut amount = rng.gen_range(1..chunk + 1);
                let data_len = s.borrow().get_data().len();
                amount = std::cmp::min(amount, replay_len - data_len);

                let mut new_data = vec![0; amount];
                if rng.gen_range(0..data_error_chance) == 0 {
                    // Small chance to introduce a deviation.
                    let err_at = rng.gen_range(0..amount);
                    new_data[err_at] = 1;
                    log::debug!("Stream {} added an error at {}", idx, data_len + err_at);
                }
                data.extend(new_data.clone());
                s.borrow_mut().add_data(&new_data);
                modified = true;
            }
            if rand::random() {
                // Randomly advance delayed data.
                let amount = rng.gen_range(1..chunk + 1);
                let mut delayed = s.borrow().get_delayed_data_len();
                delayed += amount;
                if delayed <= s.borrow().get_data().len() {
                    s.borrow_mut().set_delayed_data_len(delayed);
                    modified = true;
                }
            }

            if modified {
                strat.replay_data_updated(*token);
            }

            // Small chance to end early.
            if s.borrow().get_data().len() == replay_len || rng.gen_range(0..early_exit_chance) == 0 {
                let data_len = s.borrow().get_data().len();
                log::debug!("Stream {} ended at {}", idx, data_len);
                let delayed_data_len = s.borrow().get_delayed_data_len();
                if delayed_data_len < data_len {
                    s.borrow_mut().set_delayed_data_len(data_len);
                    strat.replay_data_updated(*token);
                }
                final_datas.push((data.clone(), idx));
                s.borrow_mut().finish();
                strat.replay_removed(*token);
            }
        }

        strat.finish();

        let mut merged_data = Vec::new();
        strat.get_merged_replay().borrow_mut().get_data().reader().read_to_end(&mut merged_data).unwrap();

        // Now, filter out streams that split off alone.
        let mut common_pfx_bundles = vec![final_datas];
        for i in 0..replay_len {
            // For each bundle
            common_pfx_bundles = common_pfx_bundles.into_iter().flat_map(|mut s| {
                //If all replays ended here, then merging could've ended here.
                if s.iter().all(|d| d.0.len() <= i) {
                    return vec![s];
                }
                // Otherwise merging continued.
                s = s.into_iter().filter(|d| d.0.len() > i).collect();

                // Split bundle based on whether the byte was changed.
                let (a, b) = s.into_iter().partition(|d| d.0[i] == 0);
                let mut ret = vec![a, b];
                ret.sort_by_key(|b: &Vec<_>| -(b.len() as isize));
                // Remove single offshoot if the other bundle has at least 2 streams.
                if ret[1].len() <= 1 && ret[0].len() >= 2 {
                    ret.pop();
                }
                ret
            }).collect();
        }
        let remaining_datas: Vec<_> = common_pfx_bundles.into_iter().flatten().collect();
        let indices: Vec<_> = remaining_datas.iter().map(|t| t.1).collect();
        log::debug!("Remaining streams: {:?}", indices);
        let datas: Vec<_> = remaining_datas.into_iter().map(|t| t.0).collect();
        assert!(datas.contains(&merged_data));
    }

    #[cfg_attr(not(feature = "fuzzing_tests"), ignore)]
    #[test]
    fn test_strategy_simple_fuzzing() {
        setup_logging();
        for i in 0..100 {
            log::debug!("Run {}", i);
            simple_fuzzing_round();
        }
    }
}

// TODO add more tests. Possibly fuzzing. Not all python server tests made sense to convert.
