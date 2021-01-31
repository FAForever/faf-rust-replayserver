use std::collections::HashMap;
use sqlx::types::time::OffsetDateTime;

use crate::error::SaveError;

use super::database::Database;

pub type GameTeams = HashMap<u16, Vec<String>>;

pub struct GameStats {
    pub featured_mod: String,
    pub game_type: String,
    pub recorder: String,   // Same as host. This used to only be in local replays. I accidentally added it server-side. Not harmful.
    pub host: String,
    pub launched_at: f64,   // Both need to be float, as the original replay server used Python's time.time()
    pub game_end: f64,
    pub title: String,
    pub mapname: String,
    pub num_players: u32,
}

// Not in database for easier testing. I guess this could be in a trait? Doesn't matter.
pub async fn get_teams_in_game(db: &Database, id: u64) -> Result<GameTeams, SaveError> {
    let players = db.get_team_players(id).await?;
    let mut res = GameTeams::new();
    for p in players.into_iter() {
        if !res.contains_key(&p.team) {
            res.insert(p.team, Vec::new());
        }
        res.get_mut(&p.team).unwrap().push(p.login);
    }
    Ok(res)
}

pub async fn get_game_stats(db: &Database, id: u64) -> Result<GameStats, SaveError> {
    let stats = db.get_game_stat_row(id).await?;
    let player_count = db.get_player_count(id).await?;

    // Mapname looks like this: maps/<stuff>.zip
    // Previous two servers extracted the stuff with path.splitext(path.basename(...)).
    // Rust unix path handling is ugly and it really won't ever be anything else than this
    // (see MapService in API), so we just trim manually.
    let mapname = stats.file_name
        .and_then(|f| f.rsplitn(2, '.').last().map(String::from))
        .and_then(|f| f.rsplitn(2, '/').nth(0).map(String::from))
        .unwrap_or("None".into());

    Ok(GameStats {
        featured_mod: stats.game_mod,
        game_type: stats.game_type,
        recorder: stats.host.clone(),
        host: stats.host,
        launched_at: stats.start_time.unix_timestamp() as f64,
        game_end: stats.end_time.unwrap_or(OffsetDateTime::now_utc()).unix_timestamp() as f64,
        title: stats.game_name,
        mapname,
        num_players: player_count,
    })
}
