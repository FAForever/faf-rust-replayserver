use std::collections::HashMap;

use crate::{database::queries::GameTeams, database::queries::Queries, error::SaveError};

// Saved replay's json header. Some fields are weird / redundant, that's legacy. TODO: new format.
pub struct ReplayJsonHeader {
    complete: bool,
    featured_mod: Option<String>,
    featured_mod_versions: GameTeams,
    game_end: i64,
    game_type: String,
    host: String,
    launched_at: i64,
    mapname: String,
    num_players: i64,
    recorder: String, // Same as host. This used to only be in local replays. I accidentally added it server-side. Not harmful.
    state: String,
    teams: HashMap<String, Vec<String>>,
    title: String,
    uid: u64,
}

impl ReplayJsonHeader {
    fn fixup_team_dict(mut d: GameTeams) -> HashMap<String, Vec<String>> {
        let mut out = HashMap::new();
        for (k, v) in d.drain() {
            out.insert(k.to_string(), v);
        }
        out
    }

    pub async fn from_id_and_db(db: &Queries, uid: u64) -> Result<ReplayJsonHeader, SaveError> {
        let game_stats = db.get_game_stats(uid).await?;
        let teams = Self::fixup_team_dict(db.get_teams_in_game(uid).await?);
        let featured_mod_versions = match &game_stats.featured_mod {
            None => HashMap::new(),
            Some(..) => db.get_teams_in_game(uid).await?,
        };

        Ok(Self {
            complete: true,
            featured_mod: game_stats.featured_mod,
            featured_mod_versions,
            game_end: game_stats.game_end,
            game_type: game_stats.game_type,
            host: game_stats.host.clone(),
            launched_at: game_stats.launched_at,
            mapname: game_stats.mapname,
            num_players: game_stats.num_players,
            recorder: game_stats.host,
            state: "PLAYING".into(),
            teams,
            title: game_stats.title,
            uid,
        })
    }
}
