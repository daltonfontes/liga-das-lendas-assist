use serde::Deserialize;
use serde_json::{json, Value};

use crate::lcu::client::LcuClient;
use crate::state::Store;

/// Prefixo das páginas criadas pelo assistente; só elas são substituídas.
pub const PAGE_PREFIX: &str = "Assist · ";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Perk {
    pub id: i64,
    pub name: String,
}

/// Página recomendada pelo próprio cliente (`/lol-perks/v1/recommended-pages/...`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RecommendedPage {
    pub position: String,
    pub is_default_position: bool,
    pub keystone: Perk,
    pub perks: Vec<Perk>,
    pub primary_perk_style_id: i64,
    pub secondary_perk_style_id: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PerkPage {
    pub id: i64,
    pub name: String,
    pub is_deletable: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Inventory {
    can_add_custom_page: bool,
}

/// Escolhe a recomendação da rota; sem ela, a rota padrão do campeão; senão a primeira.
pub fn choose_page<'a>(
    pages: &'a [RecommendedPage],
    position: &str,
) -> Option<&'a RecommendedPage> {
    pages
        .iter()
        .find(|p| p.position.eq_ignore_ascii_case(position))
        .or_else(|| pages.iter().find(|p| p.is_default_position))
        .or_else(|| pages.first())
}

/// Corpo do POST /lol-perks/v1/pages.
pub fn page_body(champion: &str, page: &RecommendedPage) -> Value {
    json!({
        "name": format!("{PAGE_PREFIX}{champion}"),
        "primaryStyleId": page.primary_perk_style_id,
        "subStyleId": page.secondary_perk_style_id,
        "selectedPerkIds": page.perks.iter().map(|p| p.id).collect::<Vec<_>>(),
        "current": true,
    })
}

/// Posição no formato do endpoint de recomendações.
pub fn api_position(position: Option<&str>) -> String {
    match position {
        Some(p) if !p.is_empty() => p.to_uppercase(),
        _ => "NONE".into(),
    }
}

/// Importa as runas recomendadas uma vez por campeão/rota durante a seleção.
#[derive(Default)]
pub struct RuneImporter {
    last: Option<(i64, String)>,
    map_id: Option<i64>,
    /// Texto mostrado no card do assistente, ex.: "Runas: Eletrocutar".
    pub status: Option<String>,
}

impl RuneImporter {
    pub fn needs_import(&self, champion_id: i64, position: &str) -> bool {
        champion_id > 0 && self.last.as_ref() != Some(&(champion_id, position.to_string()))
    }

    pub async fn import(
        &mut self,
        lcu: &LcuClient,
        store: &Store,
        champion_id: i64,
        champion: &str,
        position: Option<&str>,
    ) {
        let position = api_position(position);
        if !self.needs_import(champion_id, &position) {
            return;
        }
        // Marca antes de chamar o cliente para não repetir a cada evento se algo falhar.
        self.last = Some((champion_id, position.clone()));
        match self.try_import(lcu, champion_id, champion, &position).await {
            Ok(keystone) => {
                store.log(format!("Runas importadas para {champion}: {keystone}."));
                self.status = Some(format!("Runas: {keystone}"));
            }
            Err(err) => {
                store.log(format!("Não foi possível importar as runas: {err}"));
                self.status = Some("Runas não importadas".into());
            }
        }
    }

    async fn try_import(
        &mut self,
        lcu: &LcuClient,
        champion_id: i64,
        champion: &str,
        position: &str,
    ) -> anyhow::Result<String> {
        if self.map_id.is_none() {
            let session: Value = lcu
                .get("/lol-gameflow/v1/session")
                .await
                .unwrap_or_default();
            self.map_id = Some(session["map"]["id"].as_i64().unwrap_or(11));
        }
        let map_id = self.map_id.unwrap_or(11);
        let pages: Vec<RecommendedPage> = lcu
            .get(&format!(
                "/lol-perks/v1/recommended-pages/champion/{champion_id}/position/{position}/map/{map_id}"
            ))
            .await?;
        let page = choose_page(&pages, position)
            .ok_or_else(|| anyhow::anyhow!("o cliente não tem recomendação para {champion}"))?;

        // Substitui a página do assistente, se existir; páginas do jogador ficam intactas.
        let existing: Vec<PerkPage> = lcu.get("/lol-perks/v1/pages").await?;
        for old in existing
            .iter()
            .filter(|p| p.is_deletable && p.name.starts_with(PAGE_PREFIX))
        {
            lcu.request(
                reqwest::Method::DELETE,
                &format!("/lol-perks/v1/pages/{}", old.id),
                None,
            )
            .await?;
        }
        let inventory: Inventory = lcu.get("/lol-perks/v1/inventory").await?;
        if !inventory.can_add_custom_page {
            anyhow::bail!(
                "sem espaço para uma nova página — apague uma página de runas no cliente"
            );
        }
        lcu.post("/lol-perks/v1/pages", Some(&page_body(champion, page)))
            .await?;
        Ok(page.keystone.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(position: &str, default: bool, keystone: &str) -> RecommendedPage {
        RecommendedPage {
            position: position.into(),
            is_default_position: default,
            keystone: Perk {
                id: 1,
                name: keystone.into(),
            },
            perks: [8112, 8126, 8138, 8135, 8226, 8210, 5008, 5008, 5001]
                .into_iter()
                .map(|id| Perk {
                    id,
                    name: String::new(),
                })
                .collect(),
            primary_perk_style_id: 8100,
            secondary_perk_style_id: 8200,
        }
    }

    #[test]
    fn escolhe_a_pagina_da_rota() {
        let pages = [
            rec("TOP", false, "Conquistador"),
            rec("MIDDLE", true, "Eletrocutar"),
        ];
        assert_eq!(
            choose_page(&pages, "TOP").unwrap().keystone.name,
            "Conquistador"
        );
        assert_eq!(
            choose_page(&pages, "middle").unwrap().keystone.name,
            "Eletrocutar"
        );
        // Rota sem recomendação: usa a rota padrão do campeão.
        assert_eq!(
            choose_page(&pages, "NONE").unwrap().keystone.name,
            "Eletrocutar"
        );
        assert!(choose_page(&[], "TOP").is_none());
    }

    #[test]
    fn monta_a_pagina() {
        let body = page_body("Ahri", &rec("MIDDLE", true, "Eletrocutar"));
        assert_eq!(body["name"], "Assist · Ahri");
        assert_eq!(body["primaryStyleId"], 8100);
        assert_eq!(body["subStyleId"], 8200);
        assert_eq!(body["selectedPerkIds"].as_array().unwrap().len(), 9);
        assert_eq!(body["current"], true);
    }

    #[test]
    fn importa_uma_vez_por_campeao_e_rota() {
        let mut importer = RuneImporter::default();
        assert_eq!(api_position(Some("middle")), "MIDDLE");
        assert_eq!(api_position(None), "NONE");
        assert!(!importer.needs_import(0, "MIDDLE"));
        assert!(importer.needs_import(103, "MIDDLE"));
        importer.last = Some((103, "MIDDLE".into()));
        assert!(!importer.needs_import(103, "MIDDLE"));
        assert!(importer.needs_import(61, "MIDDLE"), "trocou de campeão");
    }

    #[test]
    fn le_a_recomendacao_do_cliente() {
        let page: RecommendedPage = serde_json::from_value(json!({
            "position": "MIDDLE", "isDefaultPosition": true,
            "keystone": { "id": 8112, "name": "Eletrocutar", "iconPath": "/x.png" },
            "perks": [{ "id": 8112, "name": "Eletrocutar" }, { "id": 5008, "name": "Força Adaptativa" }],
            "primaryPerkStyleId": 8100, "secondaryPerkStyleId": 8200,
            "summonerSpellIds": [4, 14], "recommendationId": "abc"
        }))
        .unwrap();
        assert_eq!(page.keystone.name, "Eletrocutar");
        assert_eq!(page.perks.len(), 2);
    }
}
