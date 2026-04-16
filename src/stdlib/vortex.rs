//! Vortex Bot API stdlib functions:
//! vortex_send, vortex_reply, vortex_delete, vortex_get_rooms, vortex_get_me,
//! vortex_mute, vortex_ban, vortex_kick, vortex_warn,
//! vortex_get_user, vortex_get_members, vortex_set_slow_mode

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::interpreter::SharedState;
use crate::value::Value;
use crate::error::{GravError, GravResult};

pub async fn call_vortex_builtin(
    name:   &str,
    args:   &[Value],
    shared: &Arc<Mutex<SharedState>>,
) -> GravResult<Option<Value>> {
    match name {
        "vortex_send" => {
            // vortex_send(room_id, text)
            let (room_id, text) = get_room_text(args, "vortex_send")?;
            vortex_post("/api/bot/send",
                serde_json::json!({ "room_id": room_id, "text": text }),
                shared).await?;
            Ok(Some(Value::Null))
        }
        "vortex_reply" => {
            // vortex_reply(room_id, reply_to_msg_id, text)
            if args.len() < 3 {
                return Err(GravError::Arity { name: name.into(), expected: 3, got: args.len() });
            }
            let room_id  = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_reply: room_id must be int"))?;
            let reply_to = args[1].as_int().ok_or_else(|| crate::runtime_err!("vortex_reply: reply_to must be int"))?;
            let text     = args[2].to_string();
            vortex_post("/api/bot/reply",
                serde_json::json!({ "room_id": room_id, "reply_to": reply_to, "text": text }),
                shared).await?;
            Ok(Some(Value::Null))
        }
        "vortex_delete" | "vortex_delete_message" => {
            // vortex_delete(room_id, msg_id)
            if args.len() < 2 {
                return Err(GravError::Arity { name: name.into(), expected: 2, got: args.len() });
            }
            let room_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_delete: room_id must be int"))?;
            let msg_id  = args[1].as_int().ok_or_else(|| crate::runtime_err!("vortex_delete: msg_id must be int"))?;
            vortex_post("/api/bot/delete",
                serde_json::json!({ "room_id": room_id, "message_id": msg_id }),
                shared).await?;
            Ok(Some(Value::Null))
        }
        "vortex_get_rooms" => {
            // vortex_get_rooms() -> list of room maps
            let (token, url) = {
                let st = shared.lock().await;
                (st.bot_token.clone(), st.vortex_url.clone())
            };
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("{}/api/bot/rooms", url.trim_end_matches('/')))
                .header("Authorization", format!("Bot {token}"))
                .timeout(std::time::Duration::from_secs(10))
                .send().await
                .map_err(|e| GravError::Bot(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(GravError::Bot(format!("vortex_get_rooms HTTP {}", resp.status())));
            }
            let rooms: Vec<serde_json::Value> = resp.json().await
                .map_err(|e| GravError::Bot(e.to_string()))?;
            let list = rooms.into_iter().map(json_to_value).collect();
            Ok(Some(Value::make_list(list)))
        }
        "vortex_get_me" => {
            // vortex_get_me() -> map with bot info
            let (token, url) = {
                let st = shared.lock().await;
                (st.bot_token.clone(), st.vortex_url.clone())
            };
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("{}/api/bot/me", url.trim_end_matches('/')))
                .header("Authorization", format!("Bot {token}"))
                .timeout(std::time::Duration::from_secs(10))
                .send().await
                .map_err(|e| GravError::Bot(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(GravError::Bot(format!("vortex_get_me HTTP {}", resp.status())));
            }
            let info: serde_json::Value = resp.json().await
                .map_err(|e| GravError::Bot(e.to_string()))?;
            Ok(Some(json_to_value(info)))
        }
        // ── Moderation API ────────────────────────────────────────────────────

        "vortex_mute" => {
            // vortex_mute(room_id, user_id, duration_sec)
            if args.len() < 3 {
                return Err(GravError::Arity { name: name.into(), expected: 3, got: args.len() });
            }
            let room_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_mute: room_id must be int"))?;
            let user_id = args[1].as_int().ok_or_else(|| crate::runtime_err!("vortex_mute: user_id must be int"))?;
            let dur_sec = args[2].as_int().ok_or_else(|| crate::runtime_err!("vortex_mute: duration must be int"))?;
            vortex_post("/api/bot/mute",
                serde_json::json!({ "room_id": room_id, "user_id": user_id, "duration_sec": dur_sec }),
                shared).await?;
            Ok(Some(Value::Null))
        }
        "vortex_ban" => {
            // vortex_ban(room_id, user_id)  or  vortex_ban(room_id, user_id, reason)
            if args.len() < 2 {
                return Err(GravError::Arity { name: name.into(), expected: 2, got: args.len() });
            }
            let room_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_ban: room_id must be int"))?;
            let user_id = args[1].as_int().ok_or_else(|| crate::runtime_err!("vortex_ban: user_id must be int"))?;
            let reason  = args.get(2).map(|v| v.to_string());
            vortex_post("/api/bot/ban",
                serde_json::json!({ "room_id": room_id, "user_id": user_id, "reason": reason }),
                shared).await?;
            Ok(Some(Value::Null))
        }
        "vortex_kick" => {
            // vortex_kick(room_id, user_id)
            if args.len() < 2 {
                return Err(GravError::Arity { name: name.into(), expected: 2, got: args.len() });
            }
            let room_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_kick: room_id must be int"))?;
            let user_id = args[1].as_int().ok_or_else(|| crate::runtime_err!("vortex_kick: user_id must be int"))?;
            vortex_post("/api/bot/kick",
                serde_json::json!({ "room_id": room_id, "user_id": user_id }),
                shared).await?;
            Ok(Some(Value::Null))
        }
        "vortex_warn" => {
            // vortex_warn(room_id, user_id, text)
            if args.len() < 3 {
                return Err(GravError::Arity { name: name.into(), expected: 3, got: args.len() });
            }
            let room_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_warn: room_id must be int"))?;
            let user_id = args[1].as_int().ok_or_else(|| crate::runtime_err!("vortex_warn: user_id must be int"))?;
            let text    = args[2].to_string();
            vortex_post("/api/bot/warn",
                serde_json::json!({ "room_id": room_id, "user_id": user_id, "text": text }),
                shared).await?;
            Ok(Some(Value::Null))
        }
        "vortex_set_slow_mode" => {
            // vortex_set_slow_mode(room_id, seconds)
            if args.len() < 2 {
                return Err(GravError::Arity { name: name.into(), expected: 2, got: args.len() });
            }
            let room_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_set_slow_mode: room_id must be int"))?;
            let seconds = args[1].as_int().ok_or_else(|| crate::runtime_err!("vortex_set_slow_mode: seconds must be int"))?;
            vortex_post("/api/bot/slow-mode",
                serde_json::json!({ "room_id": room_id, "seconds": seconds }),
                shared).await?;
            Ok(Some(Value::Null))
        }
        "vortex_get_user" => {
            // vortex_get_user(user_id) -> map with user info
            if args.is_empty() {
                return Err(GravError::Arity { name: name.into(), expected: 1, got: 0 });
            }
            let user_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_get_user: user_id must be int"))?;
            let (token, url) = {
                let st = shared.lock().await;
                (st.bot_token.clone(), st.vortex_url.clone())
            };
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("{}/api/bot/user/{}", url.trim_end_matches('/'), user_id))
                .header("Authorization", format!("Bot {token}"))
                .timeout(std::time::Duration::from_secs(10))
                .send().await
                .map_err(|e| GravError::Bot(e.to_string()))?;
            if !resp.status().is_success() {
                return Ok(Some(Value::Null));
            }
            let info: serde_json::Value = resp.json().await
                .map_err(|e| GravError::Bot(e.to_string()))?;
            Ok(Some(json_to_value(info)))
        }
        "vortex_get_members" => {
            // vortex_get_members(room_id) -> list of user maps
            if args.is_empty() {
                return Err(GravError::Arity { name: name.into(), expected: 1, got: 0 });
            }
            let room_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("vortex_get_members: room_id must be int"))?;
            let (token, url) = {
                let st = shared.lock().await;
                (st.bot_token.clone(), st.vortex_url.clone())
            };
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("{}/api/bot/members/{}", url.trim_end_matches('/'), room_id))
                .header("Authorization", format!("Bot {token}"))
                .timeout(std::time::Duration::from_secs(10))
                .send().await
                .map_err(|e| GravError::Bot(e.to_string()))?;
            if !resp.status().is_success() {
                return Ok(Some(Value::make_list(vec![])));
            }
            let members: Vec<serde_json::Value> = resp.json().await
                .map_err(|e| GravError::Bot(e.to_string()))?;
            let list = members.into_iter().map(json_to_value).collect();
            Ok(Some(Value::make_list(list)))
        }

        _ => Ok(None),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn get_room_text(args: &[Value], name: &str) -> GravResult<(i64, String)> {
    if args.len() < 2 {
        return Err(GravError::Arity { name: name.into(), expected: 2, got: args.len() });
    }
    let room_id = args[0].as_int().ok_or_else(|| crate::runtime_err!("{name}: room_id must be int"))?;
    let text    = args[1].to_string();
    Ok((room_id, text))
}

async fn vortex_post(path: &str, body: serde_json::Value, shared: &Arc<Mutex<SharedState>>) -> GravResult<()> {
    let (token, url) = {
        let st = shared.lock().await;
        (st.bot_token.clone(), st.vortex_url.clone())
    };
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}{}", url.trim_end_matches('/'), path))
        .header("Authorization", format!("Bot {token}"))
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send().await
        .map_err(|e| GravError::Bot(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let msg = resp.text().await.unwrap_or_default();
        eprintln!("[gravitix] vortex API error {status}: {msg}");
    }
    Ok(())
}

fn json_to_value(v: serde_json::Value) -> Value {
    use std::collections::HashMap;
    match v {
        serde_json::Value::Null            => Value::Null,
        serde_json::Value::Bool(b)         => Value::Bool(b),
        serde_json::Value::Number(n)       => {
            if let Some(i) = n.as_i64() { Value::Int(i) }
            else { Value::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s)       => Value::make_str(s),
        serde_json::Value::Array(arr)      => Value::make_list(arr.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj)     => {
            let map: HashMap<String, Value> = obj.into_iter().map(|(k, v)| (k, json_to_value(v))).collect();
            Value::make_map(map)
        }
    }
}
