use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use reqwest::Method;
use rustls::ClientConfig;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::Connector;

use super::credentials::Credentials;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LcuEvent {
    pub uri: String,
    pub event_type: String,
    #[serde(default)]
    pub data: Value,
}

// Códigos de mensagem WAMP 1.0 usados pelo LCU.
const WAMP_SUBSCRIBE: u8 = 5;
const WAMP_EVENT: u64 = 8;

pub fn http_client(tls: Arc<ClientConfig>, timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .use_preconfigured_tls((*tls).clone())
        .timeout(timeout)
        .no_proxy()
        .build()
        .expect("cliente HTTP")
}

/// Cliente da League Client Update API: REST + WebSocket (WAMP).
#[derive(Clone)]
pub struct LcuClient {
    http: reqwest::Client,
    tls: Arc<ClientConfig>,
    pub port: u16,
    auth: String,
}

impl LcuClient {
    pub fn new(creds: &Credentials, tls: Arc<ClientConfig>) -> Self {
        let token =
            base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", creds.password));
        Self {
            http: http_client(tls.clone(), Duration::from_secs(5)),
            tls,
            port: creds.port,
            auth: format!("Basic {token}"),
        }
    }

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let url = format!("https://127.0.0.1:{}{}", self.port, path);
        let mut req = self
            .http
            .request(method.clone(), &url)
            .header("Authorization", &self.auth)
            .header("Accept", "application/json");
        if let Some(body) = body {
            req = req.json(body);
        }
        let res = req
            .send()
            .await
            .with_context(|| format!("{method} {path}"))?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            bail!("{method} {path} -> {status}");
        }
        Ok(if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        Ok(serde_json::from_value(
            self.request(Method::GET, path, None).await?,
        )?)
    }

    pub async fn post(&self, path: &str, body: Option<&Value>) -> anyhow::Result<Value> {
        self.request(Method::POST, path, body).await
    }

    pub async fn patch(&self, path: &str, body: &Value) -> anyhow::Result<Value> {
        self.request(Method::PATCH, path, Some(body)).await
    }

    /// Conecta ao WebSocket e assina todos os eventos JSON. O canal fecha quando o cliente fecha.
    pub async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<LcuEvent>> {
        let mut request = format!("wss://127.0.0.1:{}/", self.port).into_client_request()?;
        request
            .headers_mut()
            .insert("Authorization", HeaderValue::from_str(&self.auth)?);
        let connector = Connector::Rustls(self.tls.clone());
        let (ws, _) =
            tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
                .await?;
        let (mut write, mut read) = ws.split();
        write
            .send(Message::text(format!(
                r#"[{WAMP_SUBSCRIBE},"OnJsonApiEvent"]"#
            )))
            .await?;

        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            // `write` precisa viver enquanto lemos, senão a conexão é encerrada.
            let _write = write;
            while let Some(Ok(msg)) = read.next().await {
                let Message::Text(text) = msg else { continue };
                if let Some(event) = parse_wamp_event(&text) {
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
            }
        });
        Ok(rx)
    }
}

/// `[8, "OnJsonApiEvent", {uri, eventType, data}]` -> evento.
pub fn parse_wamp_event(text: &str) -> Option<LcuEvent> {
    let value: Value = serde_json::from_str(text).ok()?;
    let array = value.as_array()?;
    if array.first()?.as_u64()? != WAMP_EVENT {
        return None;
    }
    serde_json::from_value(array.get(2)?.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_eventos_wamp() {
        let ev = parse_wamp_event(
            r#"[8,"OnJsonApiEvent",{"uri":"/lol-gameflow/v1/gameflow-phase","eventType":"Update","data":"ChampSelect"}]"#,
        )
        .unwrap();
        assert_eq!(ev.uri, "/lol-gameflow/v1/gameflow-phase");
        assert_eq!(ev.event_type, "Update");
        assert_eq!(ev.data, "ChampSelect");
        assert!(parse_wamp_event(r#"[0,"x",1,"y"]"#).is_none());
        assert!(parse_wamp_event("lixo").is_none());
    }
}
