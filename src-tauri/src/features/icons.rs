use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::champions::ChampionIndex;
use crate::lcu::client::LcuClient;

/// Ícones dos campeões servidos pelo próprio cliente, com cache em memória.
///
/// A janela não consegue autenticar no LCU, então o app expõe os ícones pelo
/// protocolo `champ://` (ver `lib.rs`), que chama [`ChampionIcons::icon`].
#[derive(Default)]
pub struct ChampionIcons {
    client: Mutex<Option<LcuClient>>,
    index: Mutex<Arc<ChampionIndex>>,
    cache: Mutex<HashMap<i64, Arc<Vec<u8>>>>,
}

impl ChampionIcons {
    pub fn set_client(&self, client: Option<LcuClient>) {
        *self.client.lock().unwrap() = client;
    }

    pub fn set_index(&self, index: ChampionIndex) {
        *self.index.lock().unwrap() = Arc::new(index);
    }

    /// Nomes dos campeões em ordem alfabética (autocompletar das configurações).
    pub fn names(&self) -> Vec<String> {
        self.index.lock().unwrap().names()
    }

    /// PNG do ícone de um campeão, por nome, alias ou id.
    pub async fn icon(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let id = self.index.lock().unwrap().resolve(key)?;
        if let Some(png) = self.cache.lock().unwrap().get(&id) {
            return Some(png.clone());
        }
        let client = self.client.lock().unwrap().clone()?;
        let path = format!("/lol-game-data/assets/v1/champion-icons/{id}.png");
        let png = Arc::new(client.get_bytes(&path).await.ok()?);
        self.cache.lock().unwrap().insert(id, png.clone());
        Some(png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::champions::ChampionSummary;

    #[tokio::test]
    async fn sem_cliente_ou_campeao_desconhecido_nao_ha_icone() {
        let icons = ChampionIcons::default();
        icons.set_index(ChampionIndex::new(vec![ChampionSummary {
            id: 103,
            name: "Ahri".into(),
            alias: "Ahri".into(),
        }]));
        assert_eq!(icons.names(), ["Ahri"]);
        assert!(icons.icon("Ahri").await.is_none(), "sem cliente conectado");
        assert!(icons.icon("Teemo").await.is_none(), "campeão desconhecido");
    }
}
