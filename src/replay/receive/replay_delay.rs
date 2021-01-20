use std::{cell::RefCell, collections::VecDeque};
use async_stream::stream;
use futures::{Stream, FutureExt};
use futures::stream::StreamExt;

use crate::replay::{position::StreamPosition, streams::WriterReplay};

use tokio::time::{sleep, Duration};

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
        let rd_up: u64 = ((self.delay_s * 1000) % self.sleep_ms != 0).into();
        (div + rd_up + 1) as usize
    }

    fn header_stream<'a>(&self, replay: &'a RefCell<WriterReplay>) -> impl Stream<Item = StreamUpdates> + 'a {
        async move {
            // This guarantees that we emit HEADER positions exactly once.
            let at_header = StreamPosition::DATA(0);
            // Don't borrow across an await
            let f = replay.borrow().wait(at_header);
            f.await;
            StreamUpdates::NewHeader
        }.into_stream()
    }

    fn data_update_stream<'a>(&self, replay: &'a RefCell<WriterReplay>) -> impl Stream<Item = StreamUpdates> + 'a {
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

    fn end_stream<'a>(&self, replay: &'a RefCell<WriterReplay>) -> impl Stream<Item = StreamUpdates> + 'a {
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
    pub fn track_delayed_progress<'a>(&self, replay: &'a RefCell<WriterReplay>) -> impl Stream<Item = StreamUpdates> + 'a {
        let header_stream = self.header_stream(replay);
        let data_stream = self.data_update_stream(replay);
        let end_stream = self.end_stream(replay);

        let header_and_data_stream = header_stream.chain(data_stream);
        let replay_end = replay.borrow().wait(StreamPosition::FINISHED(0));
        let capped_stream = header_and_data_stream.take_until(replay_end);
        capped_stream.chain(end_stream)
    }
}
