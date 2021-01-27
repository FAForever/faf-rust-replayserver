use std::rc::{Weak, Rc};

use futures::{Stream, StreamExt};
use tokio::join;
use weak_table::WeakValueHashMap;
use tokio_util::sync::CancellationToken;

use crate::{server::connection::Connection, config::Settings};
use crate::accept::header::ConnectionType;

use super::Replay;

struct AssignedConnection {
    connection: Connection,
    replay: Rc<Replay>,
    replay_is_newly_created: bool,
}

pub struct Replays {
    replays: WeakValueHashMap<u64, Weak<Replay>>,
    replay_builder: Box<dyn Fn(u64) -> Replay>,
}

impl Replays {
    pub fn new(replay_builder: Box<dyn Fn(u64) -> Replay>) -> Self {
        Self { replays: WeakValueHashMap::new(), replay_builder}
    }

    pub fn build(shutdown_token: CancellationToken, config: Settings) -> Self {
        let replay_builder = move |rid| {Replay::new(rid, shutdown_token.clone(), &config)};
        Self::new(Box::new(replay_builder))
    }

    fn assign_connection_to_replay(&mut self, c: Connection) -> Option<AssignedConnection> {
        let conn_header = c.get_header();
        let mut replay_is_newly_created = false;

        let maybe_replay = match self.replays.get(&conn_header.id) {
            Some(r) => Some(r),
            None => {
                if conn_header.type_ == ConnectionType::READER {
                    log::debug!("Reader connection does not have a matching replay (id {})", conn_header.id);
                    None
                } else {
                    let r = Rc::new((self.replay_builder)(conn_header.id));
                    self.replays.insert(conn_header.id, r.clone());
                    replay_is_newly_created = true;
                    Some(r)
                }
            }
        };
        match maybe_replay {
            None => None,
            Some(replay) => Some(AssignedConnection {connection: c, replay, replay_is_newly_created}),
        }
    }

    async fn handle_connection_or_replay_lifetime(mac: Option<AssignedConnection>) {
        if let Some(ac) = mac {
            if ac.replay_is_newly_created {
                join! {
                    ac.replay.handle_connection(ac.connection),
                    ac.replay.lifetime()
                };
            } else {
                ac.replay.handle_connection(ac.connection).await;
            }
        }
    }

    pub async fn handle_connections_and_replays(&mut self, cs: impl Stream<Item = Connection>) {
        cs.map(|c| self.assign_connection_to_replay(c))
          .for_each_concurrent(None, Self::handle_connection_or_replay_lifetime).await
    }
}
