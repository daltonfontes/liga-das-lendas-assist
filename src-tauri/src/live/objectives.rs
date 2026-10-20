use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Tempos de objetivos em segundos de jogo. Mudam entre patches — ajuste aqui.
pub mod times {
    pub const DRAGON_FIRST_SPAWN: f64 = 5.0 * 60.0;
    pub const DRAGON_RESPAWN: f64 = 5.0 * 60.0;
    pub const ELDER_RESPAWN: f64 = 6.0 * 60.0;
    pub const HERALD_FIRST_SPAWN: f64 = 15.0 * 60.0;
    pub const BARON_FIRST_SPAWN: f64 = 25.0 * 60.0;
    pub const BARON_RESPAWN: f64 = 6.0 * 60.0;
    pub const INHIBITOR_RESPAWN: f64 = 5.0 * 60.0;
    /// Dragões necessários para a alma (o próximo vira Ancião).
    pub const DRAGONS_FOR_SOUL: usize = 4;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Team {
    #[serde(rename = "ORDER")]
    Order,
    #[serde(rename = "CHAOS")]
    Chaos,
}

impl Team {
    pub fn enemy(self) -> Self {
        match self {
            Team::Order => Team::Chaos,
            Team::Chaos => Team::Order,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LiveEvent {
    #[serde(rename = "EventName")]
    pub name: String,
    #[serde(rename = "EventTime")]
    pub time: f64,
    #[serde(rename = "KillerName")]
    pub killer: Option<String>,
    #[serde(rename = "DragonType")]
    pub dragon_type: Option<String>,
    #[serde(rename = "InhibKilled")]
    pub inhib: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveTimer {
    pub name: String,
    /// Tempo de jogo (s) em que nasce.
    pub spawns_at: f64,
    /// Segundos restantes; 0 quando já está vivo.
    pub remaining: f64,
    pub alive: bool,
    /// Fração (0–1) do intervalo até nascer já decorrida.
    pub progress: f64,
    /// Complemento, ex.: "vermelho · meio" para um inibidor.
    pub detail: Option<String>,
}

impl ObjectiveTimer {
    /// Nome com o complemento, para as dicas.
    pub fn full_name(&self) -> String {
        match &self.detail {
            Some(d) => format!("{} ({d})", self.name),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PerTeam<T> {
    #[serde(rename = "ORDER")]
    pub order: T,
    #[serde(rename = "CHAOS")]
    pub chaos: T,
}

impl<T> PerTeam<T> {
    pub fn get(&self, team: Team) -> &T {
        match team {
            Team::Order => &self.order,
            Team::Chaos => &self.chaos,
        }
    }

    pub fn get_mut(&mut self, team: Team) -> &mut T {
        match team {
            Team::Order => &mut self.order,
            Team::Chaos => &mut self.chaos,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectiveSummary {
    pub timers: Vec<ObjectiveTimer>,
    pub dragons: PerTeam<Vec<String>>,
    pub barons: PerTeam<u32>,
}

/// Timer de um objetivo que fica indisponível de `from` até `spawns_at`.
fn timer(name: impl Into<String>, from: f64, spawns_at: f64, game_time: f64) -> ObjectiveTimer {
    let remaining = (spawns_at - game_time).max(0.0).ceil();
    let span = (spawns_at - from).max(1.0);
    ObjectiveTimer {
        name: name.into(),
        spawns_at,
        remaining,
        alive: remaining == 0.0,
        progress: ((game_time - from) / span).clamp(0.0, 1.0),
        detail: None,
    }
}

/// Calcula os timers de objetivos a partir dos eventos da Live Client Data API.
/// `team_of` mapeia o nome de quem abateu para o time (minions/torres não têm time).
pub fn compute(
    events: &[LiveEvent],
    game_time: f64,
    team_of: impl Fn(Option<&str>) -> Option<Team>,
) -> ObjectiveSummary {
    let mut dragons: PerTeam<Vec<String>> = PerTeam::default();
    let mut barons: PerTeam<u32> = PerTeam::default();
    let (mut dragon_from, mut next_dragon) = (0.0, times::DRAGON_FIRST_SPAWN);
    let (mut baron_from, mut next_baron) = (0.0, times::BARON_FIRST_SPAWN);
    let mut herald_taken = false;
    let mut inhibs: BTreeMap<String, f64> = BTreeMap::new();
    let soul = |d: &PerTeam<Vec<String>>| {
        d.order.len() >= times::DRAGONS_FOR_SOUL || d.chaos.len() >= times::DRAGONS_FOR_SOUL
    };

    let mut sorted: Vec<&LiveEvent> = events.iter().collect();
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    for ev in sorted {
        match ev.name.as_str() {
            "DragonKill" => {
                let kind = ev.dragon_type.clone().unwrap_or_else(|| "?".into());
                if let Some(team) = team_of(ev.killer.as_deref()).filter(|_| kind != "Elder") {
                    dragons.get_mut(team).push(kind);
                }
                dragon_from = ev.time;
                next_dragon = ev.time
                    + if soul(&dragons) {
                        times::ELDER_RESPAWN
                    } else {
                        times::DRAGON_RESPAWN
                    };
            }
            "BaronKill" => {
                if let Some(team) = team_of(ev.killer.as_deref()) {
                    *barons.get_mut(team) += 1;
                }
                baron_from = ev.time;
                next_baron = ev.time + times::BARON_RESPAWN;
            }
            "HeraldKill" => herald_taken = true,
            "InhibKilled" => {
                if let Some(id) = &ev.inhib {
                    inhibs.insert(id.clone(), ev.time);
                }
            }
            "InhibRespawned" => {
                if let Some(id) = &ev.inhib {
                    inhibs.remove(id);
                }
            }
            _ => {}
        }
    }

    let mut timers = vec![timer(
        if soul(&dragons) {
            "Dragão Ancião"
        } else {
            "Dragão"
        },
        dragon_from,
        next_dragon,
        game_time,
    )];
    if !herald_taken && game_time < times::BARON_FIRST_SPAWN {
        timers.push(timer("Arauto", 0.0, times::HERALD_FIRST_SPAWN, game_time));
    }
    timers.push(timer("Barão", baron_from, next_baron, game_time));
    for (id, killed_at) in inhibs {
        let respawn = killed_at + times::INHIBITOR_RESPAWN;
        if respawn > game_time {
            let mut t = timer("Inibidor", killed_at, respawn, game_time);
            t.detail = Some(describe_inhib(&id));
            timers.push(t);
        }
    }
    ObjectiveSummary {
        timers,
        dragons,
        barons,
    }
}

/// "Barracks_T1_L1" -> "azul · topo". T1 = time azul (ORDER), T2 = vermelho (CHAOS).
pub fn describe_inhib(id: &str) -> String {
    let Some(pos) = id.find("_T") else {
        return id.to_string();
    };
    let bytes = id.as_bytes();
    let side = match bytes.get(pos + 2) {
        Some(b'1') => "azul",
        Some(b'2') => "vermelho",
        _ => return id.to_string(),
    };
    let lane = match bytes.get(pos + 4) {
        Some(b'L') => "topo",
        Some(b'C') => "meio",
        Some(b'R') => "bot",
        _ => return id.to_string(),
    };
    format!("{side} · {lane}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team_of(name: Option<&str>) -> Option<Team> {
        match name? {
            n if n.starts_with("Blue") => Some(Team::Order),
            n if n.starts_with("Red") => Some(Team::Chaos),
            _ => None,
        }
    }

    fn ev(name: &str, time: f64) -> LiveEvent {
        LiveEvent {
            name: name.into(),
            time,
            ..Default::default()
        }
    }

    #[test]
    fn timers_iniciais() {
        let o = compute(&[], 120.0, team_of);
        assert_eq!(o.timers[0].name, "Dragão");
        assert_eq!(o.timers[0].remaining, times::DRAGON_FIRST_SPAWN - 120.0);
        assert!((o.timers[0].progress - 120.0 / times::DRAGON_FIRST_SPAWN).abs() < 1e-9);
        assert!(o.timers.iter().any(|t| t.name == "Arauto"));
    }

    #[test]
    fn respawn_do_dragao_e_contagem_por_time() {
        let kill = LiveEvent {
            killer: Some("BlueJg".into()),
            dragon_type: Some("Fire".into()),
            ..ev("DragonKill", 400.0)
        };
        let o = compute(&[kill], 500.0, team_of);
        assert_eq!(o.dragons.order, ["Fire"]);
        assert_eq!(o.timers[0].spawns_at, 400.0 + times::DRAGON_RESPAWN);
    }

    #[test]
    fn anciao_depois_da_alma() {
        let kills: Vec<LiveEvent> = (1..=4)
            .map(|i| LiveEvent {
                killer: Some("RedJg".into()),
                dragon_type: Some("Water".into()),
                ..ev("DragonKill", i as f64 * 400.0)
            })
            .collect();
        let o = compute(&kills, 1700.0, team_of);
        assert_eq!(o.timers[0].name, "Dragão Ancião");
        assert_eq!(o.timers[0].spawns_at, 1600.0 + times::ELDER_RESPAWN);
    }

    #[test]
    fn inibidores() {
        let kill = LiveEvent {
            inhib: Some("Barracks_T2_C1".into()),
            ..ev("InhibKilled", 1800.0)
        };
        let o = compute(&[kill], 1900.0, team_of);
        let t = o.timers.iter().find(|t| t.name == "Inibidor").unwrap();
        assert_eq!(t.full_name(), "Inibidor (vermelho · meio)");
        assert_eq!(t.remaining, 200.0);
        assert!((t.progress - 100.0 / 300.0).abs() < 1e-9);
        assert_eq!(describe_inhib("Barracks_T1_L1"), "azul · topo");
    }
}
