use async_stream::stream;
use futures::stream::StreamExt;
use futures::Stream;
use std::{cell::RefCell, collections::VecDeque};

use crate::replay::streams::WReplayRef;

use tokio::time::Duration;

use super::merge_strategy::MergeStrategy;

#[derive(Clone, Copy)]
pub enum StreamUpdates {
    NewHeader,
    DataUpdate,
    Finished,
}

pub struct StreamDelayQueue {
    sleep_ms: u64,
    history_size: usize,
    queue: VecDeque<usize>,
}

impl StreamDelayQueue {
    pub fn new(delay_s: u64, sleep_ms: u64) -> Self {
        let history_size = Self::history_size(delay_s, sleep_ms);
        Self { sleep_ms, history_size, queue: VecDeque::new() }
    }

    fn history_size(delay_s: u64, sleep_ms: u64) -> usize {
        /* Right after placing a new value, Nth element in the deque (counting from 0) spent N
         * sleep cycles in it. We want:
         *     N * sleep_ms >= delay_s * 1000
         *     N >= ceil(delay_s * 1000 / sleep_ms)
         *     Deque size >= ceil(delay_s * 1000 / sleep_ms) + 1
         *
         */
        let div = (delay_s * 1000) / sleep_ms;
        let should_round_up = (delay_s * 1000) % sleep_ms != 0;
        let round_up: u64 = should_round_up.into();
        (div + round_up + 1) as usize
    }

    pub fn push_and_get_delayed(&mut self, current_len: usize) -> usize {
        self.queue.push_back(current_len);
        if self.queue.len() > self.history_size {
            self.queue.pop_front();
        }
        *self.queue.front().unwrap()
    }

    pub async fn wait_cycle(&self) {
        tokio::time::sleep(Duration::from_millis(self.sleep_ms)).await;
    }
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

    fn header_stream(&self, replay: WReplayRef) -> impl Stream<Item = StreamUpdates> {
        stream! {
            // Don't borrow across an await
            let f = replay.borrow().wait_for_header();
            f.await;
            yield StreamUpdates::NewHeader;
        }
    }

    fn data_update_stream(&self, replay: WReplayRef) -> impl Stream<Item = StreamUpdates> {
        let mut pos_queue = StreamDelayQueue::new(self.delay_s, self.sleep_ms);
        let mut prev_current = 0;
        let mut prev_delayed = 0;
        stream! {
            loop {
                let current = replay.borrow().data_len();
                let delayed = pos_queue.push_and_get_delayed(current);
                replay.borrow_mut().set_delayed_data_progress(delayed);
                if (current, delayed) != (prev_current, prev_delayed) {
                    yield StreamUpdates::DataUpdate;
                }
                prev_current = current;
                prev_delayed = delayed;
                pos_queue.wait_cycle().await;
            }
        }
    }

    fn end_stream(&self, replay: WReplayRef) -> impl Stream<Item = StreamUpdates> {
        stream! {
            // Don't borrow across an await
            let f = replay.borrow().wait_until_finished();
            f.await;
            let final_len = replay.borrow_mut().data_len();
            replay.borrow_mut().set_delayed_data_progress(final_len);
            yield StreamUpdates::DataUpdate;
            yield StreamUpdates::Finished;
        }
    }

    fn track_delayed_progress(&self, replay: WReplayRef) -> impl Stream<Item = StreamUpdates> {
        let header_stream = self.header_stream(replay.clone());
        let data_stream = self.data_update_stream(replay.clone());
        let end_stream = self.end_stream(replay.clone());

        let header_and_data_stream = header_stream.chain(data_stream);
        let replay_end = replay.borrow().wait_until_finished();
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
