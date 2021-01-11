use std::{cell::RefCell, collections::VecDeque};
use async_stream::stream;
use futures::{Stream, FutureExt};
use futures::stream::StreamExt;

use crate::replay::streams::position::StreamPosition;

use super::writer_replay::WriterReplay;
use tokio::time::{sleep, Duration};

#[derive(Clone, Copy)]
pub struct StreamPositions {
    pub delayed: StreamPosition,
    pub current: StreamPosition,
}

impl StreamPositions {
    fn new(delayed: StreamPosition, current: StreamPosition) -> Self {
        Self { delayed, current }
    }
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

    fn header_stream<'a>(&self, replay: &'a RefCell<WriterReplay>) -> impl Stream<Item = StreamPositions> + 'a {
        async move {
            // This guarantees that we emit HEADER positions exactly once.
            let at_header = StreamPosition::DATA(0);
            // Don't borrow across an await
            let f = replay.borrow().wait(at_header);
            f.await;
            StreamPositions::new(StreamPosition::HEADER, StreamPosition::HEADER)
        }.into_stream()
    }

    fn data_stream<'a>(&self, replay: &'a RefCell<WriterReplay>) -> impl Stream<Item = StreamPositions> + 'a {
        let mut position_history: VecDeque<StreamPosition> = VecDeque::new();
        let history_size = self.history_size();
        let sleep_time = Duration::from_millis(self.sleep_ms);
        stream! {
            loop {
                let current = replay.borrow().position();
                position_history.push_back(current);
                if position_history.len() > history_size {
                    position_history.pop_front();
                }
                let delayed = *position_history.front().unwrap();

                // Sanity check
                if matches!((current, delayed), (StreamPosition::DATA(_), StreamPosition::DATA(_))) {
                    yield StreamPositions::new(delayed, current);
                }
                sleep(sleep_time).await;
            }
        }
    }

    fn end_stream<'a>(&self, replay: &'a RefCell<WriterReplay>) -> impl Stream<Item = StreamPositions> + 'a {
        async move {
            // Don't borrow across an await
            let f = replay.borrow().wait(StreamPosition::FINISHED(0));
            let finish = f.await;
            StreamPositions::new(finish, finish)
        }.into_stream()
    }

    // INVARIANTS:
    // * The stream always ends with a single pair of FINISHED values.
    // * If HEADER values appear, they appear once, first and as a pair of HEADER values.
    // * All other values are pairs of DATA values.
    pub fn delayed_progress<'a>(&self, replay: &'a RefCell<WriterReplay>) -> impl Stream<Item = StreamPositions> + 'a {
        let header_stream = self.header_stream(replay);
        let data_stream = self.data_stream(replay);
        let end_stream = self.end_stream(replay);

        let header_and_data_stream = header_stream.chain(data_stream);
        let replay_end = replay.borrow().wait(StreamPosition::FINISHED(0));
        let capped_stream = header_and_data_stream.take_until(replay_end);
        capped_stream.chain(end_stream)
    }
}
