use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::champions::ChampionIndex;
use crate::config::{AssistantConfig, ChampSelectConfig};
use crate::lcu::client::LcuClient;
use crate::state::Store;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Action {
    pub id: i64,
    pub actor_cell_id: i64,
    pub champion_id: i64,
    pub completed: bool,
    pub is_in_progress: bool,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Player {
    pub cell_id: i64,
    pub champion_id: i64,
    pub champion_pick_intent: i64,
    pub assigned_position: String,
    pub game_name: String,
    pub tag_line: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Bans {
    pub my_team_bans: Vec<i64>,
    pub their_team_bans: Vec<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Timer {
    pub phase: String,
    pub adjusted_time_left_in_phase: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Session {
    pub local_player_cell_id: i64,
    pub my_team: Vec<Player>,
    pub their_team: Vec<Player>,
    pub actions: Vec<Vec<Action>>,
    pub bans: Bans,
    pub timer: Timer,
}

impl Session {
    pub fn local_position(&self) -> Option<String> {
        self.my_team
            .iter()
            .find(|p| p.cell_id == self.local_player_cell_id)
            .map(|p| p.assigned_position.to_lowercase())
            .filter(|p| !p.is_empty())
    }

    fn actions(&self) -> impl Iterator<Item = &Action> {
        self.actions.iter().flatten()
    }

    fn unavailable(&self, kind: &str) -> HashSet<i64> {
        let mut taken: HashSet<i64> = self
            .actions()
            .filter(|a| a.completed)
            .map(|a| a.champion_id)
            .collect();
        taken.extend(
            self.bans
                .my_team_bans
                .iter()
                .chain(&self.bans.their_team_bans),
        );
        for p in self.my_team.iter().chain(&self.their_team) {
            if p.cell_id != self.local_player_cell_id {
                taken.insert(p.champion_id);
            }
        }
        // Não banir o que um aliado declarou que vai jogar.
        if kind == "ban" {
            taken.extend(
                self.my_team
                    .iter()
                    .filter(|p| p.cell_id != self.local_player_cell_id)
                    .map(|p| p.champion_pick_intent),
            );
        }
        taken.retain(|id| *id > 0);
        taken
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub action_id: i64,
    pub champion_id: i64,
    pub is_ban: bool,
    /// true: confirma (completed); false: apenas seleciona/declara intenção.
    pub lock: bool,
}

pub struct DecisionInput<'a> {
    pub session: &'a Session,
    /// Preferências já convertidas em ids, em ordem.
    pub picks: &'a [i64],
    pub bans: &'a [i64],
    pub auto_lock: bool,
    pub auto_ban: bool,
    /// Se informado, só considera estes campeões (os que a conta pode usar).
    pub pickable: Option<&'a HashSet<i64>>,
    pub bannable: Option<&'a HashSet<i64>>,
}

/// Decide a próxima ação na seleção de campeões. Função pura: não chama o cliente.
pub fn decide(input: &DecisionInput) -> Option<Decision> {
    let session = input.session;
    let mine: Vec<&Action> = session
        .actions()
        .filter(|a| {
            a.actor_cell_id == session.local_player_cell_id
                && !a.completed
                && (a.kind == "pick" || a.kind == "ban")
        })
        .collect();

    let current = mine.iter().find(|a| a.is_in_progress).copied();
    // Na fase de planejamento, declara a intenção de pick (sem confirmar).
    let planning = session.timer.phase == "PLANNING";
    let action = current.or_else(|| {
        if planning {
            mine.iter().find(|a| a.kind == "pick").copied()
        } else {
            None
        }
    })?;

    let is_ban = action.kind == "ban";
    if is_ban && !input.auto_ban {
        return None;
    }
    let taken = session.unavailable(&action.kind);
    let allowed = if is_ban {
        input.bannable
    } else {
        input.pickable
    };
    let available =
        |id: i64| id > 0 && !taken.contains(&id) && allowed.is_none_or(|set| set.contains(&id));

    // Uma seleção que ainda está disponível (feita pelo jogador ou por nós antes) é respeitada.
    let prefs = if is_ban { input.bans } else { input.picks };
    let champion_id = if available(action.champion_id) {
        action.champion_id
    } else {
        prefs.iter().copied().find(|id| available(*id))?
    };

    let lock = current.is_some() && (is_ban || input.auto_lock);
    if action.champion_id == champion_id && !lock {
        return None;
    }
    Some(Decision {
        action_id: action.id,
        champion_id,
        is_ban,
        lock,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberView {
    pub position: String,
    pub champion: Option<String>,
    pub intent: Option<String>,
    pub name: Option<String>,
    pub is_me: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampSelectView {
    pub phase: String,
    pub time_left_ms: Option<f64>,
    pub position: Option<String>,
    pub my_team: Vec<TeamMemberView>,
    pub their_team: Vec<Option<String>>,
    pub bans: Vec<String>,
}

pub fn build_view(session: &Session, champions: &ChampionIndex) -> ChampSelectView {
    let mut ban_ids: Vec<i64> = session
        .bans
        .my_team_bans
        .iter()
        .chain(&session.bans.their_team_bans)
        .copied()
        .collect();
    for a in session.actions().filter(|a| a.kind == "ban" && a.completed) {
        if !ban_ids.contains(&a.champion_id) {
            ban_ids.push(a.champion_id);
        }
    }
    ChampSelectView {
        phase: session.timer.phase.clone(),
        time_left_ms: session.timer.adjusted_time_left_in_phase,
        position: session.local_position(),
        my_team: session
            .my_team
            .iter()
            .map(|p| TeamMemberView {
                position: p.assigned_position.to_lowercase(),
                champion: champions.name(p.champion_id),
                intent: champions.name(p.champion_pick_intent),
                name: (!p.game_name.is_empty()).then(|| format!("{}#{}", p.game_name, p.tag_line)),
                is_me: p.cell_id == session.local_player_cell_id,
            })
            .collect(),
        their_team: session
            .their_team
            .iter()
            .map(|p| champions.name(p.champion_id))
            .collect(),
        bans: ban_ids
            .into_iter()
            .filter(|id| *id > 0)
            .map(|id| champions.name(id).unwrap_or_else(|| id.to_string()))
            .collect(),
    }
}

/// Executa as decisões na sessão de seleção de campeões.
#[derive(Default)]
pub struct ChampSelectAssistant {
    last_applied: Option<Decision>,
    pickable: Option<HashSet<i64>>,
    bannable: Option<HashSet<i64>>,
}

impl ChampSelectAssistant {
    pub fn reset(&mut self, store: &Store) {
        *self = Self::default();
        store.update(|s| s.champ_select = None);
    }

    pub async fn handle(
        &mut self,
        session: Session,
        lcu: &LcuClient,
        store: &Store,
        champions: &ChampionIndex,
        config: &AssistantConfig,
    ) {
        store.update(|s| s.champ_select = Some(build_view(&session, champions)));
        let cfg = &config.champ_select;
        if !cfg.enabled {
            return;
        }
        if self.pickable.is_none() {
            self.pickable = lcu
                .get::<Vec<i64>>("/lol-champ-select/v1/pickable-champion-ids")
                .await
                .ok()
                .filter(|v| !v.is_empty())
                .map(HashSet::from_iter);
            self.bannable = lcu
                .get::<Vec<i64>>("/lol-champ-select/v1/bannable-champion-ids")
                .await
                .ok()
                .filter(|v| !v.is_empty())
                .map(HashSet::from_iter);
        }

        let position = session.local_position();
        let resolve = |list| -> Vec<i64> {
            ChampSelectConfig::preferences(list, position.as_deref())
                .into_iter()
                .filter_map(|n| champions.resolve(n))
                .collect()
        };
        let (picks, bans) = (resolve(&cfg.picks), resolve(&cfg.bans));
        let Some(decision) = decide(&DecisionInput {
            session: &session,
            picks: &picks,
            bans: &bans,
            auto_lock: cfg.auto_lock,
            auto_ban: cfg.auto_ban,
            pickable: self.pickable.as_ref(),
            bannable: self.bannable.as_ref(),
        }) else {
            return;
        };
        if self.last_applied.as_ref() == Some(&decision) {
            return;
        }

        let mut body = json!({ "championId": decision.champion_id });
        if decision.lock {
            body["completed"] = json!(true);
        }
        let path = format!(
            "/lol-champ-select/v1/session/actions/{}",
            decision.action_id
        );
        let name = champions
            .name(decision.champion_id)
            .unwrap_or_else(|| decision.champion_id.to_string());
        match lcu.patch(&path, &body).await {
            Ok(_) => {
                let verb = if decision.is_ban {
                    "Banido"
                } else if decision.lock {
                    "Confirmado"
                } else {
                    "Selecionado"
                };
                store.log(format!("{verb}: {name}"));
                self.last_applied = Some(decision);
            }
            Err(err) => store.log(format!("Erro na seleção de campeões: {err}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::champions::ChampionSummary;

    const AHRI: i64 = 103;
    const ZED: i64 = 238;
    const YASUO: i64 = 157;
    const LEE: i64 = 64;
    const ORIANNA: i64 = 61;

    fn player(cell_id: i64, position: &str) -> Player {
        Player {
            cell_id,
            assigned_position: position.into(),
            ..Default::default()
        }
    }

    fn action(id: i64, actor: i64, kind: &str) -> Action {
        Action {
            id,
            actor_cell_id: actor,
            kind: kind.into(),
            ..Default::default()
        }
    }

    fn in_progress(mut a: Action) -> Action {
        a.is_in_progress = true;
        a
    }

    fn session(actions: Vec<Vec<Action>>) -> Session {
        Session {
            local_player_cell_id: 2,
            my_team: vec![player(0, "top"), player(1, "jungle"), player(2, "middle")],
            their_team: vec![player(5, "")],
            actions,
            timer: Timer {
                phase: "BAN_PICK".into(),
                adjusted_time_left_in_phase: None,
            },
            ..Default::default()
        }
    }

    fn run(
        s: &Session,
        auto_lock: bool,
        auto_ban: bool,
        pickable: Option<&HashSet<i64>>,
    ) -> Option<Decision> {
        decide(&DecisionInput {
            session: s,
            picks: &[AHRI, ORIANNA],
            bans: &[ZED, YASUO],
            auto_lock,
            auto_ban,
            pickable,
            bannable: None,
        })
    }

    #[test]
    fn seleciona_o_primeiro_pick_sem_confirmar() {
        let s = session(vec![vec![in_progress(action(10, 2, "pick"))]]);
        assert_eq!(
            run(&s, false, true, None),
            Some(Decision {
                action_id: 10,
                champion_id: AHRI,
                is_ban: false,
                lock: false
            })
        );
        assert!(run(&s, true, true, None).unwrap().lock);
    }

    #[test]
    fn pula_campeoes_banidos() {
        let mut ban = action(1, 5, "ban");
        ban.champion_id = AHRI;
        ban.completed = true;
        let s = session(vec![vec![ban], vec![in_progress(action(10, 2, "pick"))]]);
        assert_eq!(run(&s, false, true, None).unwrap().champion_id, ORIANNA);
    }

    #[test]
    fn respeita_campeoes_da_conta() {
        let s = session(vec![vec![in_progress(action(10, 2, "pick"))]]);
        let owned = HashSet::from([ORIANNA]);
        assert_eq!(
            run(&s, false, true, Some(&owned)).unwrap().champion_id,
            ORIANNA
        );
    }

    #[test]
    fn nao_sobrescreve_selecao_manual() {
        let mut pick = in_progress(action(10, 2, "pick"));
        pick.champion_id = LEE;
        let s = session(vec![vec![pick]]);
        assert_eq!(run(&s, false, true, None), None);
        assert_eq!(
            run(&s, true, true, None),
            Some(Decision {
                action_id: 10,
                champion_id: LEE,
                is_ban: false,
                lock: true
            })
        );
    }

    #[test]
    fn nao_bane_intencao_do_aliado() {
        let mut s = session(vec![vec![in_progress(action(3, 2, "ban"))]]);
        s.my_team[0].champion_pick_intent = ZED;
        assert_eq!(
            run(&s, false, true, None),
            Some(Decision {
                action_id: 3,
                champion_id: YASUO,
                is_ban: true,
                lock: true
            })
        );
        assert_eq!(run(&s, false, false, None), None);
    }

    #[test]
    fn declara_intencao_no_planejamento() {
        let mut s = session(vec![vec![action(10, 2, "pick")]]);
        s.timer.phase = "PLANNING".into();
        assert_eq!(
            run(&s, true, true, None),
            Some(Decision {
                action_id: 10,
                champion_id: AHRI,
                is_ban: false,
                lock: false
            })
        );
    }

    #[test]
    fn espera_a_sua_vez() {
        let s = session(vec![vec![
            in_progress(action(10, 1, "pick")),
            action(11, 2, "pick"),
        ]]);
        assert_eq!(run(&s, false, true, None), None);
    }

    #[test]
    fn le_a_sessao_do_cliente_e_monta_a_visao() {
        let s: Session = serde_json::from_value(serde_json::json!({
            "localPlayerCellId": 0,
            "myTeam": [{ "cellId": 0, "championId": AHRI, "championPickIntent": 0, "assignedPosition": "MIDDLE", "gameName": "Eu", "tagLine": "BR1" }],
            "theirTeam": [{ "cellId": 5, "championId": 0 }],
            "actions": [[{ "id": 1, "actorCellId": 5, "championId": LEE, "completed": true, "isInProgress": false, "type": "ban" }]],
            "timer": { "phase": "BAN_PICK", "adjustedTimeLeftInPhase": 21000.0 },
            "extraField": true
        }))
        .unwrap();
        let index = ChampionIndex::new(vec![
            ChampionSummary {
                id: AHRI,
                name: "Ahri".into(),
                alias: "Ahri".into(),
            },
            ChampionSummary {
                id: LEE,
                name: "Lee Sin".into(),
                alias: "LeeSin".into(),
            },
        ]);
        let view = build_view(&s, &index);
        assert_eq!(view.position.as_deref(), Some("middle"));
        assert_eq!(view.my_team[0].champion.as_deref(), Some("Ahri"));
        assert!(view.my_team[0].is_me);
        assert_eq!(view.bans, ["Lee Sin"]);
    }
}
