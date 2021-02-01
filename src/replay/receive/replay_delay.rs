use async_stream::stream;
use futures::stream::StreamExt;
use futures::{FutureExt, Stream};
use std::{cell::RefCell, collections::VecDeque};

use crate::replay::{position::StreamPosition, streams::WReplayRef};

use tokio::time::{sleep, Duration};

use super::merge_strategy::MergeStrategy;

#[derive(Clone, Copy)]
pub enum StreamUpdates {
    NewHeader,
    DataUpdate,
    Finished,
}

pub struct StreamDelay {
    delay_s: u64,
    sleep_ms: u64,
}

// This does two things:
// * Sets writer stream's delayed data position. The position is updated roughly each sleep_ms
//   miliseconds and is set to data position delay_s seconds ago. Once the replay ends, the delayed
//   position is set to real data position (since we don't need to anti-spoiler a replay that
//   already ended).
// * Produces an async stream of status updates on the replay used to drive the replay merge
//   strategy.
impl StreamDelay {
    pub fn new(delay_s: u64, sleep_ms: u64) -> Self {
        Self { delay_s, sleep_ms }
    }

    fn history_size(&self) -> usize {
        /* Right after placing a new value, Nth element in the deque (counting from 0) spent N
         * sleep cycles in it. We want:
         *     N * sleep_ms >= delay_s * 1000
         *     N >= ceil(delay_s * 1000 / sleep_ms)
         *     Deque size >= ceil(delay_s * 1000 / sleep_ms) + 1
         *
         */
        let div = (self.delay_s * 1000) / self.sleep_ms;
        let should_round_up = (self.delay_s * 1000) % self.sleep_ms != 0;
        let round_up: u64 = should_round_up.into();
        (div + round_up + 1) as usize
    }

    fn header_stream(&self, replay: WReplayRef) -> impl Stream<Item = StreamUpdates> {
        async move {
            // This guarantees that we emit HEADER positions exactly once.
            let at_header = StreamPosition::DATA(0);
            // Don't borrow across an await
            let f = replay.borrow().wait(at_header);
            f.await;
            StreamUpdates::NewHeader
        }
        .into_stream()
    }

    fn data_update_stream(&self, replay: WReplayRef) -> impl Stream<Item = StreamUpdates> {
        let mut position_history: VecDeque<StreamPosition> = VecDeque::new();
        let history_size = self.history_size();
        let sleep_time = Duration::from_millis(self.sleep_ms);
        let mut prev_current = StreamPosition::START;
        let mut prev_delayed = StreamPosition::START;
        stream! {
            loop {
                let current = replay.borrow().position();
                position_history.push_back(current);
                if position_history.len() > history_size {
                    position_history.pop_front();
                }
                let delayed = *position_history.front().unwrap();

                if current == prev_current || delayed == prev_delayed {
                    continue;
                }
                if !matches!(current, StreamPosition::DATA(..)) {   // Sanity check
                    continue;
                }
                prev_current = current;
                prev_delayed = delayed;
                replay.borrow_mut().set_delayed_data_progress(delayed.len());
                yield StreamUpdates::DataUpdate;
                sleep(sleep_time).await;
            }
        }
    }

    fn end_stream(&self, replay: WReplayRef) -> impl Stream<Item = StreamUpdates> {
        stream! {
            // Don't borrow across an await
            let f = replay.borrow().wait(StreamPosition::FINISHED(0));
            let finish = f.await;
            replay.borrow_mut().set_delayed_data_progress(finish.len());
            yield StreamUpdates::DataUpdate;
            yield StreamUpdates::Finished;
        }
    }

    // Value order:
    // * At most one NewHeader,
    // * Any number of DataUpdate,
    // * One DataUpdate after data reached final length,
    // * One Finished.
    // * NOTE: this modifies the delayed data position of the writer replay.
    fn track_delayed_progress(&self, replay: WReplayRef) -> impl Stream<Item = StreamUpdates> {
        let header_stream = self.header_stream(replay.clone());
        let data_stream = self.data_update_stream(replay.clone());
        let end_stream = self.end_stream(replay.clone());

        let header_and_data_stream = header_stream.chain(data_stream);
        let replay_end = replay.borrow().wait(StreamPosition::FINISHED(0));
        let capped_stream = header_and_data_stream.take_until(replay_end);
        capped_stream.chain(end_stream)
    }

    pub async fn update_delayed_data_and_drive_merge_strategy(
        &self,
        replay: WReplayRef,
        strategy: &RefCell<impl MergeStrategy>,
    ) {
        let token = strategy.borrow_mut().replay_added(replay.clone());
        self.track_delayed_progress(replay)
            .for_each(|p| async move {
                let mut s = strategy.borrow_mut();
                match p {
                    StreamUpdates::NewHeader => s.replay_header_added(token),
                    StreamUpdates::DataUpdate => s.replay_data_updated(token),
                    StreamUpdates::Finished => s.replay_removed(token),
                }
            })
            .await;
    }
}
