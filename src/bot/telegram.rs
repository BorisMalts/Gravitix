use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use futures_util::StreamExt;

use crate::error::{GravError, GravResult};
use crate::value::BotOutput;
use super::backend::{BotBackend, BotInfo};

// ─────────────────────────────────────────────────────────────────────────────
// Vortex Bot API update types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum VortexUpdate {
    Message {
        room_id:    i64,
        sender:     String,
        sender_id:  i64,
        text:       String,
        message_id: i64,
        #[serde(default)]
        timestamp:  i64,
    },
    Command {
        command:    String,
        #[serde(default)]
        args:       Vec<String>,
        room_id:    i64,
        sender:     String,
        sender_id:  i64,
        message_id: i64,
        #[serde(default)]
        timestamp:  i64,
    },
    Callback {
        callback_id: String,
        data:        String,
        room_id:     i64,
        sender:      String,
        sender_id:   i64,
        #[serde(default)]
        timestamp:   i64,
    },
    Join {
        room_id:  i64,
        user_id:  i64,
        username: String,
        #[serde(default)]
        timestamp: i64,
    },
    Leave {
        room_id:  i64,
        user_id:  i64,
        username: String,
        #[serde(default)]
        timestamp: i64,
    },
    Reaction {
        room_id:   i64,
        sender:    String,
        sender_id: i64,
        emoji:     String,
        #[serde(default)]
        timestamp: i64,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Vortex HTTP client
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct VortexClient {
    pub base_url: String,
    pub token:    String,
    client:       reqwest::Client,
}

impl VortexClient {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token:    token.into(),
            client:   reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(35))
                .build()
                .expect("reqwest client"),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    fn auth(&self) -> String {
        format!("Bot {}", self.token)
    }

    pub async fn connect_ws(&self) -> GravResult<impl futures_util::Stream<Item = GravResult<VortexUpdate>>> {
        use tokio_tungstenite::connect_async;

        let ws_url = self.base_url.replace("http://", "ws://").replace("https://", "wss://");
        let ws_url = format!("{}/ws/bot", ws_url.trim_end_matches('/'));

        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&ws_url)
            .header("Authorization", self.auth())
            .body(())
            .map_err(|e| GravError::Bot(e.to_string()))?;

        let (ws_stream, _) = connect_async(request).await
            .map_err(|e| GravError::Bot(format!("WebSocket connect: {e}")))?;

        Ok(ws_stream.filter_map(|msg| async move {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    serde_json::from_str::<VortexUpdate>(&text).ok().map(Ok)
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    Some(Err(GravError::Bot("WebSocket closed".into())))
                }
                Err(e) => Some(Err(GravError::Bot(e.to_string()))),
                _ => None,
            }
        }))
    }
}

#[async_trait(?Send)]
impl BotBackend for VortexClient {
    async fn get_updates(&self, timeout: u64) -> GravResult<Vec<VortexUpdate>> {
        let resp = self.client
            .get(self.url("/api/bot/updates"))
            .header("Authorization", self.auth())
            .query(&[("timeout", timeout)])
            .send().await
            .map_err(|e| GravError::Bot(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(GravError::Bot(format!("get_updates HTTP {status}: {body}")));
        }

        let updates: Vec<VortexUpdate> = resp.json().await
            .map_err(|e| GravError::Bot(format!("get_updates parse: {e}")))?;
        Ok(updates)
    }

    async fn send_output(&self, output: &BotOutput) -> GravResult<()> {
        match output {
            BotOutput::Send { room_id, text } => {
                let body = json!({ "room_id": room_id, "text": text });
                let resp = self.client
                    .post(self.url("/api/bot/send"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await
                    .map_err(|e| GravError::Bot(e.to_string()))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    eprintln!("[gravitix] send HTTP {status}: {body}");
                }
                Ok(())
            }
            BotOutput::Reply { room_id, reply_to, text } => {
                let body = json!({ "room_id": room_id, "reply_to": reply_to, "text": text });
                let resp = self.client
                    .post(self.url("/api/bot/reply"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await
                    .map_err(|e| GravError::Bot(e.to_string()))?;
                if !resp.status().is_success() {
                    eprintln!("[gravitix] reply error: {}", resp.status());
                }
                Ok(())
            }
            BotOutput::Keyboard { room_id, text, buttons } => {
                // Convert to Vortex format: [[{"text": "...", "callback_data": "..."}]]
                let vortex_buttons: Vec<Vec<serde_json::Value>> = buttons.iter()
                    .map(|row| row.iter()
                        .map(|(label, data)| json!({"text": label, "callback_data": data}))
                        .collect())
                    .collect();
                let body = json!({
                    "room_id": room_id,
                    "text": text,
                    "buttons": vortex_buttons
                });
                let resp = self.client
                    .post(self.url("/api/bot/send-keyboard"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await
                    .map_err(|e| GravError::Bot(e.to_string()))?;
                if !resp.status().is_success() {
                    eprintln!("[gravitix] send_keyboard error: {}", resp.status());
                }
                Ok(())
            }
            BotOutput::AnswerCallback { callback_id, text } => {
                let body = json!({ "callback_id": callback_id, "text": text });
                let resp = self.client
                    .post(self.url("/api/bot/callback"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await
                    .map_err(|e| GravError::Bot(e.to_string()))?;
                if !resp.status().is_success() {
                    eprintln!("[gravitix] answer_callback error: {}", resp.status());
                }
                Ok(())
            }
            BotOutput::DeleteMessage { room_id, msg_id } => {
                let body = json!({ "room_id": room_id, "message_id": msg_id });
                let resp = self.client
                    .post(self.url("/api/bot/delete"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await
                    .map_err(|e| GravError::Bot(e.to_string()))?;
                if !resp.status().is_success() {
                    eprintln!("[gravitix] delete_message error: {}", resp.status());
                }
                Ok(())
            }
            BotOutput::FederatedSend { target, text } => {
                // Parse "room@node" format
                let parts: Vec<&str> = target.splitn(2, '@').collect();
                if parts.len() == 2 {
                    let room = parts[0];
                    let node = parts[1];
                    let url = format!("http://{}/api/bot/federated_send", node);
                    let body = serde_json::json!({ "room": room, "text": text });
                    if let Err(e) = self.client.post(&url)
                        .header("Authorization", self.auth())
                        .json(&body)
                        .send().await
                    {
                        eprintln!("[gravitix] federated_send to {target} failed: {e}");
                    }
                } else {
                    eprintln!("[gravitix] federated_send: invalid target '{target}' (expected room@node)");
                }
                Ok(())
            }
            BotOutput::Typing { room_id } => {
                let body = json!({ "room_id": room_id, "action": "typing" });
                let _ = self.client
                    .post(self.url("/api/bot/typing"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::PinMsg { room_id, msg_id } => {
                let body = json!({ "room_id": room_id, "message_id": msg_id });
                let _ = self.client
                    .post(self.url("/api/bot/pin"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::UnpinMsg { room_id, msg_id } => {
                let body = json!({ "room_id": room_id, "message_id": msg_id });
                let _ = self.client
                    .post(self.url("/api/bot/unpin"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::MuteUser { room_id, user_id, duration_ms } => {
                let mut body = json!({ "room_id": room_id, "user_id": user_id });
                if let Some(d) = duration_ms { body["duration_ms"] = json!(d); }
                let _ = self.client
                    .post(self.url("/api/bot/mute"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::Embed { room_id, html, url, height, title } => {
                let mut body = json!({ "room_id": room_id, "height": height, "title": title });
                if let Some(h) = html { body["html"] = json!(h); }
                if let Some(u) = url  { body["url"]  = json!(u); }
                let _ = self.client
                    .post(self.url("/api/bot/embed"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::Notify { user_id, text } => {
                let body = json!({ "user_id": user_id, "text": text });
                let _ = self.client
                    .post(self.url("/api/bot/notify"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::NotifyRoom { room_id, text } => {
                let body = json!({ "room_id": room_id, "text": text });
                let _ = self.client
                    .post(self.url("/api/bot/notify_room"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::RichMessage { room_id, title, text, image, buttons } => {
                let vortex_buttons: Vec<Vec<serde_json::Value>> = buttons.iter()
                    .map(|row| row.iter()
                        .map(|(label, data)| json!({"text": label, "callback_data": data}))
                        .collect())
                    .collect();
                let mut body = json!({
                    "room_id": room_id,
                    "buttons": vortex_buttons
                });
                if let Some(t) = title { body["title"] = json!(t); }
                if let Some(t) = text  { body["text"]  = json!(t); }
                if let Some(i) = image { body["image"] = json!(i); }
                let resp = self.client
                    .post(self.url("/api/bot/send"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await
                    .map_err(|e| GravError::Bot(e.to_string()))?;
                if !resp.status().is_success() {
                    eprintln!("[gravitix] rich_message error: {}", resp.status());
                }
                Ok(())
            }
            // New output types — send as plain text for now
            BotOutput::Form { room_id, fields, submit } => {
                let form_text = fields.iter()
                    .map(|(name, kind)| format!("  [{kind}] {name}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let text = format!("Form:\n{form_text}\n[{submit}]");
                let body = json!({ "room_id": room_id, "text": text });
                let _ = self.client
                    .post(self.url("/api/bot/send"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::Table { room_id, text } | BotOutput::StreamChunk { room_id, text } => {
                let body = json!({ "room_id": room_id, "text": text });
                let _ = self.client
                    .post(self.url("/api/bot/send"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::BanUser { room_id, user_id, reason } => {
                let mut body = json!({ "room_id": room_id, "user_id": user_id });
                if let Some(r) = reason { body["reason"] = json!(r); }
                let _ = self.client
                    .post(self.url("/api/bot/ban"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::KickUser { room_id, user_id } => {
                let body = json!({ "room_id": room_id, "user_id": user_id });
                let _ = self.client
                    .post(self.url("/api/bot/kick"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::SetSlowMode { room_id, seconds } => {
                let body = json!({ "room_id": room_id, "seconds": seconds });
                let _ = self.client
                    .post(self.url("/api/bot/slow-mode"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
            BotOutput::UiUpdate { variable, value } => {
                let body = json!({ "type": "ui_update", "variable": variable, "value": value });
                let _ = self.client
                    .post(self.url("/api/bot/send"))
                    .header("Authorization", self.auth())
                    .json(&body)
                    .send().await;
                Ok(())
            }
        }
    }

    async fn get_me(&self) -> GravResult<BotInfo> {
        let resp = self.client
            .get(self.url("/api/bot/me"))
            .header("Authorization", self.auth())
            .send().await
            .map_err(|e| GravError::Bot(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(GravError::Bot(format!("get_me HTTP {}", resp.status())));
        }

        let info: BotInfo = resp.json().await
            .map_err(|e| GravError::Bot(format!("get_me parse: {e}")))?;
        Ok(info)
    }
}
