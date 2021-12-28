use std::rc::{Rc, Weak};

use futures::{Stream, StreamExt, stream};
use tokio_util::sync::CancellationToken;
use weak_table::WeakValueHashMap;

use super::{save::ReplaySaver, Replay};
use crate::error::ConnectionError;
use crate::{accept::header::ConnectionType, metrics};
use crate::{config::Settings, server::connection::Connection};

enum Assignment {
    Connection(Connection, Rc<Replay>),
    NewReplay(Rc<Replay>),
}

pub struct Replays {
    replays: WeakValueHashMap<u64, Weak<Replay>>,
    new_replay: Box<dyn Fn(u64) -> Replay>,
}

impl Replays {
    pub fn new(shutdown_token: CancellationToken, config: Settings, saver: ReplaySaver) -> Self {
        let replay_builder = move |rid| Replay::new(rid, shutdown_token.clone(), config.clone(), saver.clone());
        Self {
            replays: WeakValueHashMap::new(),
            new_replay: Box::new(replay_builder),
        }
    }

    fn assign_connection_to_replay(&mut self, c: Connection) -> Vec<Assignment> {
        let conn_header = c.get_header();
        let mut assignments = vec![];

        let replay = match self.replays.get(&conn_header.id) {
            Some(r) => r,
            None => {
                if conn_header.type_ == ConnectionType::Reader {
                    log::info!("{} asked for replay {}, which is not running", c, conn_header.id);
                    metrics::inc_served_conns(Some(ConnectionError::CannotAssignToReplay));
                    return vec![];
                }
                let r = Rc::new((self.new_replay)(conn_header.id));
                self.replays.insert(conn_header.id, r.clone());
                assignments.push(Assignment::NewReplay(r.clone()));
                r
            },
        };
        assignments.push(Assignment::Connection(c, replay));
        assignments
    }

    async fn handle_connection_or_replay_lifetime(a: Assignment) {
        match a {
            Assignment::NewReplay(r) => r.lifetime().await,
            Assignment::Connection(c, r) => {
                let res = r.handle_connection(c).await;
                metrics::inc_served_conns(res.err());
            }
        }
    }

    pub async fn handle_connections_and_replays(&mut self, cs: impl Stream<Item = Connection>) {
        cs.flat_map(|c| stream::iter(self.assign_connection_to_replay(c)))
            .for_each_concurrent(None, Self::handle_connection_or_replay_lifetime)
            .await
    }
}
