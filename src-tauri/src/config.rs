use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssistantConfig {
    pub auto_accept: AutoAcceptConfig,
    pub champ_select: ChampSelectConfig,
    pub live_game: LiveGameConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoAcceptConfig {
    pub enabled: bool,
    /// Espera antes de aceitar, para dar tempo de recusar.
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChampSelectConfig {
    pub enabled: bool,
    /// Confirma o campeão automaticamente depois de selecioná-lo.
    pub auto_lock: bool,
    /// Bane automaticamente quando for a sua vez.
    pub auto_ban: bool,
    /// Rota ("top", "jungle", "middle", "bottom", "utility" ou "default") -> campeões
    /// (nome ou alias) em ordem de preferência.
    pub picks: HashMap<String, Vec<String>>,
    pub bans: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveGameConfig {
    pub enabled: bool,
    pub poll_interval_ms: u64,
}

impl Default for AutoAcceptConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_ms: 2000,
        }
    }
}

impl Default for LiveGameConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval_ms: 2000,
        }
    }
}

impl ChampSelectConfig {
    /// Lista da rota seguida da lista "default".
    pub fn preferences<'a>(
        list: &'a HashMap<String, Vec<String>>,
        position: Option<&str>,
    ) -> Vec<&'a str> {
        let by_position = position.and_then(|p| list.get(p)).into_iter().flatten();
        let fallback = list.get("default").into_iter().flatten();
        by_position.chain(fallback).map(String::as_str).collect()
    }
}

pub fn load(path: &Path) -> AssistantConfig {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
            eprintln!(
                "[config] {} inválido ({err}); usando padrões",
                path.display()
            );
            AssistantConfig::default()
        }),
        Err(_) => AssistantConfig::default(),
    }
}

pub fn save(path: &Path, config: &AssistantConfig) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(config)? + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campos_ausentes_usam_padroes() {
        let cfg: AssistantConfig =
            serde_json::from_str(r#"{"champSelect":{"enabled":true}}"#).unwrap();
        assert!(cfg.champ_select.enabled);
        assert!(cfg.auto_accept.enabled);
        assert_eq!(cfg.auto_accept.delay_ms, 2000);
    }

    #[test]
    fn preferencias_da_rota_antes_do_default() {
        let mut picks = HashMap::new();
        picks.insert("middle".into(), vec!["Ahri".into()]);
        picks.insert("default".into(), vec!["Malphite".into()]);
        assert_eq!(
            ChampSelectConfig::preferences(&picks, Some("middle")),
            ["Ahri", "Malphite"]
        );
        assert_eq!(ChampSelectConfig::preferences(&picks, None), ["Malphite"]);
    }
}
