use lazy_static::lazy_static;
use prometheus_exporter::prometheus::{
    register_int_counter, register_int_counter_vec, register_int_gauge, register_int_gauge_vec, IntCounter,
    IntCounterVec, IntGauge, IntGaugeVec,
};

use crate::error::{ConnResult, ConnectionError};

lazy_static! {
    pub static ref ACTIVE_CONNS: IntGaugeVec = register_int_gauge_vec!(
        "replayserver_active_connections_count",
        "Count of currently active connections.",
        &["category"]
    )
    .unwrap();
    pub static ref SERVED_CONNS: IntCounterVec = register_int_counter_vec!(
        "replayserver_served_connections_total",
        "How many connections we served to completion.",
        &["result"]
    )
    .unwrap();
    pub static ref RUNNING_REPLAYS: IntGauge = register_int_gauge!(
        "replayserver_running_replays_count",
        "Count of currently running replays."
    )
    .unwrap();
    pub static ref FINISHED_REPLAYS: IntCounter = register_int_counter!(
        "replayserver_finished_replays_total",
        "Number of replays ran to completion."
    )
    .unwrap();
    pub static ref SAVED_REPLAYS: IntCounter = register_int_counter!(
        "replayserver_saved_replay_files_total",
        "Total replays successfully saved to disk."
    )
    .unwrap();
}

pub fn inc_served_conns<T>(res: &ConnResult<T>) {
    let label = match res {
        Ok(..) => "Success",
        Err(e) => match e {
            ConnectionError::NoData => "Empty connection",
            ConnectionError::BadData(..) => "Bad data",
            ConnectionError::IO { .. } => "I/O error",
            ConnectionError::CannotAssignToReplay => "No replay matched",
        },
    };
    SERVED_CONNS.with_label_values(&[label]).inc();
}
