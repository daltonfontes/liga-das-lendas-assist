use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::features::champ_select::ChampSelectView;
use crate::live::live_game::LiveGameView;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summoner {
    pub game_name: String,
    pub tag_line: String,
    pub level: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub time: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantState {
    pub connected: bool,
    pub summoner: Option<Summoner>,
    pub phase: String,
    pub champ_select: Option<ChampSelectView>,
    pub live_game: Option<LiveGameView>,
    pub log: Vec<LogEntry>,
}

impl Default for AssistantState {
    fn default() -> Self {
        Self {
            connected: false,
            summoner: None,
            phase: "None".into(),
            champ_select: None,
            live_game: None,
            log: Vec::new(),
        }
    }
}

type Emitter = Box<dyn Fn(&AssistantState) + Send + Sync>;

/// Estado observável: cada alteração é enviada para a janela.
pub struct Store {
    state: Mutex<AssistantState>,
    emit: Emitter,
}

const MAX_LOG: usize = 50;

impl Store {
    pub fn new(emit: impl Fn(&AssistantState) + Send + Sync + 'static) -> Self {
        Self {
            state: Mutex::new(AssistantState::default()),
            emit: Box::new(emit),
        }
    }

    pub fn get(&self) -> AssistantState {
        self.state.lock().unwrap().clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut AssistantState)) {
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            f(&mut state);
            state.clone()
        };
        (self.emit)(&snapshot);
    }

    pub fn log(&self, message: impl Into<String>) {
        let message = message.into();
        println!("[assist] {message}");
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.update(|s| {
            s.log.push(LogEntry { time, message });
            if s.log.len() > MAX_LOG {
                s.log.remove(0);
            }
        });
    }
}
