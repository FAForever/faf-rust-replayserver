use std::rc::{Rc, Weak};

use async_stream::stream;
use futures::{Stream, StreamExt};
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
        let replay_builder =
            move |rid| Replay::new(rid, shutdown_token.clone(), config.clone(), saver.clone());
        Self {
            replays: WeakValueHashMap::new(),
            new_replay: Box::new(replay_builder),
        }
    }

    fn assign_connection_to_replay(&mut self, c: Connection) -> impl Stream<Item = Assignment> {
        let conn_header = c.get_header();
        let mut replay_is_newly_created = false;

        let maybe_replay = match self.replays.get(&conn_header.id) {
            Some(r) => Ok(r),
            None => {
                if conn_header.type_ == ConnectionType::Reader {
                    log::debug!("Replay id {} not found for {}", conn_header.id, c);
                    Err(ConnectionError::CannotAssignToReplay)
                } else {
                    let r = Rc::new((self.new_replay)(conn_header.id));
                    self.replays.insert(conn_header.id, r.clone());
                    replay_is_newly_created = true;
                    Ok(r)
                }
            }
        };
        stream! {
            let replay = match maybe_replay {
                Err(e) => {
                    metrics::inc_served_conns::<()>(&Err(e));
                    return;
                },
                Ok(r) => r,
            };
            if replay_is_newly_created {
                yield Assignment::NewReplay(replay.clone());
            }
            yield Assignment::Connection(c, replay);
        }
    }

    async fn handle_connection_or_replay_lifetime(a: Assignment) {
        match a {
            Assignment::NewReplay(r) => r.lifetime().await,
            Assignment::Connection(c, r) => {
                let res = r.handle_connection(c).await;
                metrics::inc_served_conns(&res);
            }
        }
    }

    pub async fn handle_connections_and_replays(&mut self, cs: impl Stream<Item = Connection>) {
        cs.flat_map(|c| self.assign_connection_to_replay(c))
            .for_each_concurrent(None, Self::handle_connection_or_replay_lifetime)
            .await
    }
}
