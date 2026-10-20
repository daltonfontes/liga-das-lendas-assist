use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::config::AssistantConfig;
use crate::features::auto_accept::{AutoAccept, ReadyCheck};
use crate::features::champ_select::{ChampSelectAssistant, Session};
use crate::features::champions::{ChampionIndex, ChampionSummary};
use crate::lcu::client::{LcuClient, LcuEvent};
use crate::lcu::credentials::Finder;
use crate::lcu::tls::riot_client_config;
use crate::live::live_game::LiveGameTracker;
use crate::state::{Store, Summoner};

fn phase_pt(phase: &str) -> &str {
    match phase {
        "None" => "No cliente",
        "Lobby" => "No lobby",
        "Matchmaking" => "Na fila",
        "ReadyCheck" => "Partida encontrada",
        "ChampSelect" => "Seleção de campeões",
        "GameStart" => "Carregando partida",
        "InProgress" => "Em partida",
        "Reconnect" => "Reconectar",
        "WaitingForStats" => "Aguardando estatísticas",
        "PreEndOfGame" | "EndOfGame" => "Fim de jogo",
        other => other,
    }
}

/// Liga o assistente ao cliente do LoL e reconecta quando o cliente é reaberto.
pub async fn run(store: Arc<Store>, config: Arc<RwLock<AssistantConfig>>) {
    let tls = match riot_client_config() {
        Ok(tls) => tls,
        Err(err) => return store.log(format!("Falha ao carregar o certificado da Riot: {err}")),
    };
    let mut finder = Finder::default();
    let mut live = LiveGameTracker::new(tls.clone());
    let mut warned = false;
    loop {
        let Some(creds) = finder.find() else {
            if !warned {
                store.log("Aguardando o cliente do League of Legends abrir...");
                warned = true;
            }
            sleep(Duration::from_secs(3)).await;
            continue;
        };
        warned = false;
        let lcu = LcuClient::new(&creds, tls.clone());
        // O processo pode existir antes de a API subir.
        if serve_client(lcu, store.clone(), config.clone(), &mut live)
            .await
            .is_err()
        {
            sleep(Duration::from_secs(2)).await;
        }
    }
}

/// Atende um cliente conectado até ele fechar. Erro se o WebSocket não conectar.
pub async fn serve_client(
    lcu: LcuClient,
    store: Arc<Store>,
    config: Arc<RwLock<AssistantConfig>>,
    live: &mut LiveGameTracker,
) -> anyhow::Result<()> {
    let events = lcu.subscribe().await?;
    ClientSession::new(lcu, store, config, live)
        .run(events)
        .await;
    Ok(())
}

/// Uma conexão com o cliente, do login até ele fechar.
struct ClientSession<'a> {
    lcu: LcuClient,
    store: Arc<Store>,
    config: Arc<RwLock<AssistantConfig>>,
    live: &'a mut LiveGameTracker,
    champions: ChampionIndex,
    champ_select: ChampSelectAssistant,
    auto_accept: AutoAccept,
    phase: Option<String>,
}

impl<'a> ClientSession<'a> {
    fn new(
        lcu: LcuClient,
        store: Arc<Store>,
        config: Arc<RwLock<AssistantConfig>>,
        live: &'a mut LiveGameTracker,
    ) -> Self {
        Self {
            lcu,
            store,
            config,
            live,
            champions: ChampionIndex::default(),
            champ_select: ChampSelectAssistant::default(),
            auto_accept: AutoAccept::default(),
            phase: None,
        }
    }

    fn config(&self) -> AssistantConfig {
        self.config.read().unwrap().clone()
    }

    async fn run(mut self, mut events: mpsc::Receiver<LcuEvent>) {
        self.store.icons.set_client(Some(self.lcu.clone()));
        self.store.update(|s| s.connected = true);
        self.store
            .log(format!("Conectado ao cliente (porta {}).", self.lcu.port));
        self.load_summoner().await;
        self.load_champions().await;
        let phase = self
            .lcu
            .get::<String>("/lol-gameflow/v1/gameflow-phase")
            .await
            .unwrap_or_else(|_| "None".into());
        self.on_phase(phase).await;

        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => self.on_event(event).await,
                    None => break,
                },
                // Retomou a automação: reavalia a seleção sem esperar o próximo evento.
                _ = self.store.resumed.notified() => {
                    if let Ok(session) = self.lcu.get::<Session>("/lol-champ-select/v1/session").await {
                        self.handle_session(session).await;
                    }
                }
            }
        }

        self.auto_accept.cancel();
        self.live.stop();
        self.store.icons.set_client(None);
        self.store.update(|s| {
            s.connected = false;
            s.phase = "None".into();
            s.summoner = None;
            s.champ_select = None;
            s.live_game = None;
        });
        self.store.log("Cliente fechado.");
    }

    async fn on_event(&mut self, event: LcuEvent) {
        let deleted = event.event_type == "Delete";
        match event.uri.as_str() {
            "/lol-gameflow/v1/gameflow-phase" => {
                if let Some(phase) = event.data.as_str() {
                    self.on_phase(phase.to_string()).await;
                }
            }
            "/lol-matchmaking/v1/ready-check" => {
                let check = if deleted {
                    None
                } else {
                    ReadyCheck::deserialize(&event.data).ok()
                };
                let cfg = self.config().auto_accept;
                self.auto_accept.handle(
                    check,
                    cfg.enabled,
                    Duration::from_millis(cfg.delay_ms),
                    &self.lcu,
                    &self.store,
                );
            }
            "/lol-champ-select/v1/session" => {
                if deleted {
                    self.champ_select.reset(&self.store);
                } else if let Ok(session) = Session::deserialize(&event.data) {
                    self.handle_session(session).await;
                }
            }
            "/lol-summoner/v1/current-summoner" => self.load_summoner().await,
            _ => {}
        }
    }

    async fn handle_session(&mut self, session: Session) {
        let config = self.config();
        self.champ_select
            .handle(session, &self.lcu, &self.store, &self.champions, &config)
            .await;
    }

    async fn on_phase(&mut self, phase: String) {
        if self.phase.as_deref() == Some(phase.as_str()) {
            return;
        }
        self.phase = Some(phase.clone());
        self.store.update(|s| s.phase = phase.clone());
        self.store.log(format!("Fase: {}", phase_pt(&phase)));

        if phase == "ChampSelect" {
            if self.champions.is_empty() {
                self.load_champions().await;
            }
            // Busca a sessão atual (útil ao abrir o assistente no meio da seleção).
            if let Ok(session) = self
                .lcu
                .get::<Session>("/lol-champ-select/v1/session")
                .await
            {
                self.handle_session(session).await;
            }
        } else {
            self.champ_select.reset(&self.store);
        }

        match phase.as_str() {
            "GameStart" | "InProgress" | "Reconnect" => {
                self.live.start(self.store.clone(), self.config.clone())
            }
            // Mantém o último placar visível na tela de fim de jogo.
            "WaitingForStats" | "PreEndOfGame" | "EndOfGame" => self.live.stop(),
            _ => {
                self.live.stop();
                self.store.update(|s| s.live_game = None);
            }
        }
    }

    async fn load_summoner(&self) {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Current {
            game_name: String,
            tag_line: String,
            summoner_level: u32,
        }
        // Falha enquanto o cliente ainda está na tela de login.
        if let Ok(s) = self
            .lcu
            .get::<Current>("/lol-summoner/v1/current-summoner")
            .await
        {
            let summoner = Summoner {
                game_name: s.game_name,
                tag_line: s.tag_line,
                level: s.summoner_level,
            };
            self.store.update(|state| state.summoner = Some(summoner));
        }
    }

    async fn load_champions(&mut self) {
        match self
            .lcu
            .get::<Vec<ChampionSummary>>("/lol-game-data/assets/v1/champion-summary.json")
            .await
        {
            Ok(list) => {
                self.champions = ChampionIndex::new(list.clone());
                self.store.icons.set_index(ChampionIndex::new(list));
            }
            Err(err) => self.store.log(format!(
                "Não foi possível carregar a lista de campeões: {err}"
            )),
        }
    }
}
