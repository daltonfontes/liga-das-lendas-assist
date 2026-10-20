use std::sync::{Arc, RwLock};
use std::time::Duration;

use rustls::ClientConfig;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use super::objectives::{self, LiveEvent, ObjectiveSummary, PerTeam, Team};
use crate::config::AssistantConfig;
use crate::lcu::client::http_client;
use crate::state::Store;

/// Endereço fixo da Live Client Data API (só existe durante a partida).
const ALL_GAME_DATA_URL: &str = "https://127.0.0.1:2999/liveclientdata/allgamedata";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Scores {
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub creep_score: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Item {
    pub price: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePlayer {
    pub champion_name: String,
    #[serde(default)]
    pub riot_id: String,
    #[serde(default)]
    pub riot_id_game_name: String,
    #[serde(default)]
    pub summoner_name: String,
    pub team: Team,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub is_dead: bool,
    #[serde(default)]
    pub respawn_timer: f64,
    #[serde(default)]
    pub position: String,
    #[serde(default)]
    pub scores: Scores,
    #[serde(default)]
    pub items: Vec<Item>,
}

impl LivePlayer {
    fn display_name(&self) -> &str {
        [&self.riot_id, &self.riot_id_game_name, &self.summoner_name]
            .into_iter()
            .find(|n| !n.is_empty())
            .map_or("", |n| n)
    }

    fn matches(&self, name: &str) -> bool {
        !name.is_empty()
            && [&self.riot_id, &self.riot_id_game_name, &self.summoner_name]
                .iter()
                .any(|n| !n.is_empty() && (*n == name || n.split('#').next() == Some(name)))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ActivePlayer {
    pub riot_id: String,
    pub riot_id_game_name: String,
    pub summoner_name: String,
    pub level: u32,
    pub current_gold: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Events {
    #[serde(rename = "Events")]
    pub events: Vec<LiveEvent>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameData {
    pub game_mode: String,
    pub game_time: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllGameData {
    #[serde(default)]
    pub active_player: ActivePlayer,
    pub all_players: Vec<LivePlayer>,
    #[serde(default)]
    pub events: Events,
    #[serde(default)]
    pub game_data: GameData,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePlayerView {
    pub champion: String,
    pub name: String,
    pub level: u32,
    pub kda: String,
    pub cs: u32,
    pub cs_per_min: f64,
    pub item_gold: u32,
    pub dead: bool,
    pub respawn_in: u32,
    pub position: String,
    pub is_me: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamView {
    pub kills: u32,
    pub item_gold: u32,
    pub players: Vec<LivePlayerView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Me {
    pub champion: String,
    pub gold: u32,
    pub level: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveGameView {
    pub game_time: f64,
    pub game_mode: String,
    pub my_team: Team,
    pub me: Option<Me>,
    pub teams: PerTeam<TeamView>,
    pub objectives: ObjectiveSummary,
    pub tips: Vec<String>,
}

/// Transforma o JSON de /liveclientdata/allgamedata no que a janela mostra.
pub fn build_view(data: &AllGameData) -> LiveGameView {
    let game_time = data.game_data.game_time;
    let minutes = (game_time / 60.0).max(1.0 / 60.0);
    let ap = &data.active_player;
    let my_name = [&ap.riot_id, &ap.riot_id_game_name, &ap.summoner_name]
        .into_iter()
        .find(|n| !n.is_empty())
        .map_or("", |n| n);
    let me_index = data.all_players.iter().position(|p| p.matches(my_name));
    let my_team = me_index.map_or(Team::Order, |i| data.all_players[i].team);

    let mut teams: PerTeam<TeamView> = PerTeam::default();
    for (i, p) in data.all_players.iter().enumerate() {
        let item_gold: u32 = p.items.iter().map(|it| it.price * it.count.max(1)).sum();
        let team = teams.get_mut(p.team);
        team.kills += p.scores.kills;
        team.item_gold += item_gold;
        team.players.push(LivePlayerView {
            champion: p.champion_name.clone(),
            name: p.display_name().to_string(),
            level: p.level,
            kda: format!(
                "{}/{}/{}",
                p.scores.kills, p.scores.deaths, p.scores.assists
            ),
            cs: p.scores.creep_score,
            cs_per_min: (p.scores.creep_score as f64 / minutes * 10.0).round() / 10.0,
            item_gold,
            dead: p.is_dead,
            respawn_in: p.respawn_timer.max(0.0).ceil() as u32,
            position: p.position.to_uppercase(),
            is_me: Some(i) == me_index,
        });
    }

    let team_of = |killer: Option<&str>| {
        data.all_players
            .iter()
            .find(|p| p.matches(killer.unwrap_or("")))
            .map(|p| p.team)
    };
    let objectives = objectives::compute(&data.events.events, game_time, team_of);
    let tips = build_tips(&teams, my_team, &objectives, game_time);
    LiveGameView {
        game_time,
        game_mode: data.game_data.game_mode.clone(),
        my_team,
        me: me_index.map(|i| Me {
            champion: data.all_players[i].champion_name.clone(),
            gold: ap.current_gold.max(0.0) as u32,
            level: ap.level,
        }),
        teams,
        objectives,
        tips,
    }
}

fn build_tips(
    teams: &PerTeam<TeamView>,
    my_team: Team,
    objectives: &ObjectiveSummary,
    game_time: f64,
) -> Vec<String> {
    let mut tips = Vec::new();
    let (mine, enemy) = (teams.get(my_team), teams.get(my_team.enemy()));
    let dead_enemies: Vec<&LivePlayerView> = enemy.players.iter().filter(|p| p.dead).collect();
    let dead_allies = mine.players.iter().filter(|p| p.dead).count();

    for t in objectives
        .timers
        .iter()
        .filter(|t| !t.alive && t.remaining <= 60.0)
    {
        tips.push(format!(
            "{} nasce em {}s — prepare visão e prioridade de rota.",
            t.full_name(),
            t.remaining
        ));
    }
    if dead_enemies.len() >= 2 && dead_enemies.len() > dead_allies {
        let window = dead_enemies.iter().map(|p| p.respawn_in).min().unwrap_or(0);
        let alive: Vec<&str> = objectives
            .timers
            .iter()
            .filter(|t| t.alive)
            .map(|t| t.name.as_str())
            .collect();
        let target = if alive.is_empty() {
            "torres".to_string()
        } else {
            alive.join(" / ")
        };
        tips.push(format!(
            "{} inimigos mortos (~{window}s): janela para {target}.",
            dead_enemies.len()
        ));
    }
    if dead_allies >= 3 && dead_allies > dead_enemies.len() {
        tips.push(format!(
            "{dead_allies} aliados mortos: jogue seguro e evite lutas."
        ));
    }
    if let Some(me) = mine.players.iter().find(|p| p.is_me) {
        if game_time > 600.0 && me.cs_per_min < 6.0 && me.position != "UTILITY" {
            tips.push(format!(
                "Seu farm está em {} CS/min — tente chegar a 7+.",
                me.cs_per_min
            ));
        }
    }
    let diff = mine.item_gold as i64 - enemy.item_gold as i64;
    if diff >= 3000 {
        tips.push(format!(
            "Vocês estão {diff} de ouro em itens à frente: force objetivos."
        ));
    } else if diff <= -3000 {
        tips.push(format!(
            "Vocês estão {} de ouro em itens atrás: farme e defenda.",
            -diff
        ));
    }
    tips
}

/// Faz polling da Live Client Data API enquanto a partida estiver em andamento.
pub struct LiveGameTracker {
    tls: Arc<ClientConfig>,
    task: Option<JoinHandle<()>>,
}

impl LiveGameTracker {
    pub fn new(tls: Arc<ClientConfig>) -> Self {
        Self { tls, task: None }
    }

    pub fn start(&mut self, store: Arc<Store>, config: Arc<RwLock<AssistantConfig>>) {
        if self.task.is_some() || !config.read().unwrap().live_game.enabled {
            return;
        }
        store.log("Partida em andamento: acompanhando dados ao vivo.");
        let http = http_client(self.tls.clone(), Duration::from_millis(1500));
        self.task = Some(tokio::spawn(async move {
            loop {
                if let Ok(res) = http.get(ALL_GAME_DATA_URL).send().await {
                    // A API responde erro/HTML durante a tela de carregamento.
                    if let Ok(data) = res.json::<AllGameData>().await {
                        let view = build_view(&data);
                        store.update(|s| s.live_game = Some(view));
                    }
                }
                let interval = config.read().unwrap().live_game.poll_interval_ms.max(500);
                tokio::time::sleep(Duration::from_millis(interval)).await;
            }
        }));
    }

    pub fn stop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn player(name: &str, team: &str, dead: bool, respawn: f64, price: u32) -> serde_json::Value {
        json!({
            "championName": name, "riotId": format!("{name}#BR1"), "riotIdGameName": name, "team": team,
            "level": 10, "isDead": dead, "respawnTimer": respawn, "position": "MIDDLE",
            "scores": { "kills": 1, "deaths": 1, "assists": 1, "creepScore": 100, "wardScore": 5.0 },
            "items": [{ "itemID": 1, "displayName": "Item", "price": price, "count": 1, "slot": 0 }]
        })
    }

    #[test]
    fn identifica_jogador_soma_ouro_e_gera_dicas() {
        let data: AllGameData = serde_json::from_value(json!({
            "activePlayer": { "riotId": "Ahri#BR1", "level": 11, "currentGold": 1234.7, "championStats": {} },
            "allPlayers": [
                player("Ahri", "ORDER", false, 0.0, 5000),
                player("Zed", "CHAOS", true, 22.3, 1000),
                player("Lux", "CHAOS", true, 30.0, 1000),
            ],
            "events": { "Events": [{ "EventID": 0, "EventName": "GameStart", "EventTime": 0.0 }] },
            "gameData": { "gameMode": "CLASSIC", "gameTime": 600.0, "mapNumber": 11 }
        }))
        .unwrap();
        let view = build_view(&data);
        assert_eq!(view.my_team, Team::Order);
        let me = view.me.unwrap();
        assert_eq!(
            (me.champion.as_str(), me.gold, me.level),
            ("Ahri", 1234, 11)
        );
        assert_eq!(view.teams.order.item_gold, 5000);
        let zed = &view.teams.chaos.players[0];
        assert!(zed.dead);
        assert_eq!((zed.respawn_in, zed.cs_per_min), (23, 10.0));
        assert!(
            view.tips.iter().any(|t| t.contains("2 inimigos mortos")),
            "{:?}",
            view.tips
        );
        assert!(
            view.tips
                .iter()
                .any(|t| t.contains("3000 de ouro em itens à frente")),
            "{:?}",
            view.tips
        );
    }
}
