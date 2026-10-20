use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ChampionSummary {
    pub id: i64,
    pub name: String,
    pub alias: String,
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Índice de campeões vindo de `/lol-game-data/assets/v1/champion-summary.json`.
#[derive(Debug, Default)]
pub struct ChampionIndex {
    by_id: HashMap<i64, String>,
    by_key: HashMap<String, i64>,
}

impl ChampionIndex {
    pub fn new(champions: Vec<ChampionSummary>) -> Self {
        let mut index = Self::default();
        for c in champions.into_iter().filter(|c| c.id > 0) {
            index.by_key.insert(normalize(&c.name), c.id);
            index.by_key.insert(normalize(&c.alias), c.id);
            index.by_id.insert(c.id, c.name);
        }
        index
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Nomes em ordem alfabética.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.by_id.values().cloned().collect();
        names.sort();
        names
    }

    pub fn name(&self, id: i64) -> Option<String> {
        self.by_id.get(&id).cloned()
    }

    /// Aceita nome ("Lee Sin"), alias ("LeeSin") ou id numérico ("64").
    pub fn resolve(&self, name_or_id: &str) -> Option<i64> {
        match name_or_id.trim().parse::<i64>() {
            Ok(id) => self.by_id.contains_key(&id).then_some(id),
            Err(_) => self.by_key.get(&normalize(name_or_id)).copied(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_nomes_aliases_e_ids() {
        let index = ChampionIndex::new(vec![
            ChampionSummary {
                id: -1,
                name: "None".into(),
                alias: "None".into(),
            },
            ChampionSummary {
                id: 64,
                name: "Lee Sin".into(),
                alias: "LeeSin".into(),
            },
            ChampionSummary {
                id: 145,
                name: "Kai'Sa".into(),
                alias: "Kaisa".into(),
            },
        ]);
        assert_eq!(index.resolve("lee sin"), Some(64));
        assert_eq!(index.resolve("LeeSin"), Some(64));
        assert_eq!(index.resolve("kaisa"), Some(145));
        assert_eq!(index.resolve("64"), Some(64));
        assert_eq!(index.resolve("Teemo"), None);
        assert_eq!(index.resolve("None"), None);
    }
}
