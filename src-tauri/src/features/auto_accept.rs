use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::task::JoinHandle;

use crate::lcu::client::LcuClient;
use crate::state::Store;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReadyCheck {
    pub state: String,
    pub player_response: String,
}

pub fn should_accept(check: Option<&ReadyCheck>) -> bool {
    check.is_some_and(|c| c.state == "InProgress" && c.player_response == "None")
}

/// Aceita a fila automaticamente quando a partida é encontrada.
#[derive(Default)]
pub struct AutoAccept {
    pending: Option<JoinHandle<()>>,
}

impl AutoAccept {
    pub fn handle(
        &mut self,
        check: Option<ReadyCheck>,
        enabled: bool,
        delay: Duration,
        lcu: &LcuClient,
        store: &Arc<Store>,
    ) {
        if !should_accept(check.as_ref()) {
            // Fila cancelada ou já respondida (inclusive manualmente): não aceita mais.
            self.cancel();
            return;
        }
        if !enabled || self.pending.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }
        store.log(format!(
            "Partida encontrada! Aceitando em {:.1}s...",
            delay.as_secs_f64()
        ));
        let (lcu, store) = (lcu.clone(), store.clone());
        self.pending = Some(tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            match lcu
                .post("/lol-matchmaking/v1/ready-check/accept", None)
                .await
            {
                Ok(_) => store.log("Partida aceita."),
                Err(err) => store.log(format!("Não foi possível aceitar: {err}")),
            }
        }));
    }

    pub fn cancel(&mut self) {
        if let Some(task) = self.pending.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(state: &str, response: &str) -> ReadyCheck {
        ReadyCheck {
            state: state.into(),
            player_response: response.into(),
        }
    }

    #[test]
    fn so_aceita_em_andamento_sem_resposta() {
        assert!(should_accept(Some(&check("InProgress", "None"))));
        assert!(!should_accept(Some(&check("InProgress", "Declined"))));
        assert!(!should_accept(Some(&check("Invalid", "None"))));
        assert!(!should_accept(None));
    }
}
