use crate::replay::{streams::MReplayRef, streams::WReplayRef};

// This represents a way to merge replays into one canonical replay. We define:
// * A set of replays R.
//   * A replay has a header, data and a delayed position in the data. A header can be added, data
//     can be appended, delayed position can increase.
//   * The strategy is called when a replay is added, finished, a replay's header is added, a
//     replay's data or delayed data changes. Note that, because of task scheduler, we can only
//     guarantee that e.g. replay does have a header, or did finish. At each call, state of every
//     replay could've advanced arbitrarily.
//   * A replay is first added, then its header is added, then its data and delayed data change,
//     then it is marked as finished.
//   * Before calling replay_removed(), replay_data_update() will be called one last time with all
//     replay data present.
//  * A canonical replay C.
//    * The strategy sets C's header, writes data to C, sets C's delayed position and finishes C
//      based on R.
//    * Details are left to the strategy. Some obvious invariants have to be kept - adding the
//      header first, not moving delayed position beyond real data, not inventing data from thin
//      air and not doing anything after finishing C.
//  * After all replays are added, processed and removed, finish() is called. At finish(),
//    strategy should merge all outstanding data.
pub trait MergeStrategy {
    /* We use IDs to identify replays for simplicity, */
    fn replay_added(&mut self, w: WReplayRef) -> u64;
    fn replay_removed(&mut self, id: u64);
    fn replay_header_added(&mut self, id: u64);
    fn replay_data_updated(&mut self, id: u64);
    fn finish(&mut self);
    fn get_merged_replay(&self) -> MReplayRef; // FIXME change return type?
}
