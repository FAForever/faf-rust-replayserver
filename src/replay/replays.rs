use std::rc::{Weak, Rc};

use async_stream::stream;
use futures::{Stream, future::Either, StreamExt};
use tokio::sync::mpsc::Receiver;
use weak_table::WeakValueHashMap;
use tokio_util::sync::CancellationToken;

use crate::server::connection::Connection;
use crate::accept::header::ConnectionType;

use super::Replay;

type NewReplayOrConnection = Either<Rc<Replay>, (Connection, Rc<Replay>)>;

fn replays_and_connections(mut s: Receiver<Connection>,
                           replay_builder: impl Fn(u64) -> Replay) -> impl Stream<Item = NewReplayOrConnection> {
    let mut replays = WeakValueHashMap::<u64, Weak<Replay>>::new();
    stream! {
        while let Some(conn) = s.recv().await {
            let rid = conn.get_header().id;
            let replay = match replays.get(&rid) {
                Some(r) => r,
                None => {
                    if conn.get_header().type_ == ConnectionType::READER {
                        log::debug!("Reader connection does not have a matching replay (id {})", rid);
                        continue;
                    }
                    let r = Rc::new(replay_builder(rid));
                    replays.insert(rid, r.clone());
                    yield Either::Left(r.clone());
                    r
                }
            };
            yield Either::Right((conn, replay));
        }
    }
}

async fn run_one(s: NewReplayOrConnection) {
    match s {
        Either::Left(c) => c.lifetime().await,
        Either::Right(args) => {
            let (c, r) = args;
            r.handle_connection(c).await
        }
    }
}

async fn handle_connections(conns: Receiver<Connection>,
                            replay_builder: impl Fn(u64) -> Replay) {
    let s = replays_and_connections(conns, replay_builder);
    s.for_each_concurrent(None, run_one).await
}

pub struct Replays {
    shutdown_token: CancellationToken,
}

impl Replays {
    pub fn new(shutdown_token: CancellationToken) -> Self
    {
        Replays {shutdown_token}
    }

    pub async fn handle_connections(&mut self, connection_stream: Receiver<Connection>) {
        handle_connections(connection_stream, |rid| {Replay::new(rid, self.shutdown_token.clone())}).await
    }
}
