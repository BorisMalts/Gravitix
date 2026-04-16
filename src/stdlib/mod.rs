mod strings;
mod math;
mod collections;
mod convert;
mod state_fns;
mod io;
mod time;
mod vortex;
mod linalg;
mod number_theory;
mod stats;
mod transforms;
mod complex;
mod calculus;
mod special_fns;
pub mod db;
pub mod ai;
pub mod crypto;
pub mod http;
pub mod json;
pub mod regex_fns;
pub mod utils;

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::interpreter::SharedState;
use crate::value::Value;
use crate::error::GravResult;

pub async fn call_builtin(
    name:   &str,
    args:   &[Value],
    shared: &Arc<Mutex<SharedState>>,
) -> GravResult<Option<Value>> {
    if let Some(v) = convert::call_convert_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = strings::call_string_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = math::call_math_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = complex::call_complex_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = calculus::call_calculus_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = special_fns::call_special_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = collections::call_collections_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = linalg::call_linalg_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = number_theory::call_number_theory_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = stats::call_stats_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = transforms::call_transforms_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = io::call_io_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = time::call_time_builtin(name, args)? {
        return Ok(Some(v));
    }
    if let Some(v) = state_fns::call_state_builtin(name, args, shared).await? {
        return Ok(Some(v));
    }
    if let Some(v) = vortex::call_vortex_builtin(name, args, shared).await? {
        return Ok(Some(v));
    }
    if let Some(v) = db::call_db(name, args, shared).await? {
        return Ok(Some(v));
    }
    // Feature 1: AI builtins
    if let Some(v) = ai::call_ai_builtin(name, args).await? {
        return Ok(Some(v));
    }
    // Feature 12: Crypto builtins
    if let Some(v) = crypto::call_crypto_builtin(name, args)? {
        return Ok(Some(v));
    }
    // Feature 5: HTTP builtins
    if let Some(v) = http::call_http_builtin(name, args).await? {
        return Ok(Some(v));
    }
    // Feature 6: JSON builtins
    if let Some(v) = json::call_json_builtin(name, args)? {
        return Ok(Some(v));
    }
    // Feature 7: Regex builtins
    if let Some(v) = regex_fns::call_regex_builtin(name, args)? {
        return Ok(Some(v));
    }
    // Feature 8: Utility builtins
    if let Some(v) = utils::call_utils_builtin(name, args).await? {
        return Ok(Some(v));
    }
    // Feature 5 (new): fmt() — string formatting
    if name == "fmt" {
        if args.len() >= 2 {
            let template = args[0].to_string();
            let data = &args[1];
            let result = match data {
                Value::Map(m) => {
                    let m = m.borrow();
                    let mut s = template;
                    for (k, v) in m.iter() {
                        s = s.replace(&format!("{{{k}}}"), &v.to_string());
                    }
                    s
                }
                Value::List(l) => {
                    let l = l.borrow();
                    let mut s = template;
                    for (i, v) in l.iter().enumerate() {
                        s = s.replace(&format!("{{{i}}}"), &v.to_string());
                    }
                    s
                }
                _ => template,
            };
            return Ok(Some(Value::make_str(result)));
        }
        return Ok(Some(Value::make_str(args.first().map(|v| v.to_string()).unwrap_or_default())));
    }
    // Feature 6: Ok(val) and Err(msg) constructors
    if name == "Ok" {
        let val = args.first().cloned().unwrap_or(Value::Null);
        let mut m = std::collections::HashMap::new();
        m.insert("__result__".to_string(), Value::make_str("ok"));
        m.insert("value".to_string(), val);
        return Ok(Some(Value::make_map(m)));
    }
    if name == "Err" {
        let err = args.first().cloned().unwrap_or(Value::Null);
        let mut m = std::collections::HashMap::new();
        m.insert("__result__".to_string(), Value::make_str("err"));
        m.insert("error".to_string(), err);
        return Ok(Some(Value::make_map(m)));
    }
    // Feature 9 (new): notify(user_id, text) and notify_room(room_id, text)
    if name == "notify" || name == "notify_room" {
        // Return a special action marker for exec to intercept
        let mut m = std::collections::HashMap::new();
        m.insert("__action__".to_string(), Value::make_str(name));
        if let Some(id) = args.first() {
            m.insert("arg0".to_string(), id.clone());
        }
        if let Some(text) = args.get(1) {
            m.insert("arg1".to_string(), text.clone());
        }
        return Ok(Some(Value::make_map(m)));
    }
    // Feature 11: db.find("collection") — query builder
    if name == "db.find" || name == "db_find" {
        let collection = args.first().map(|v| v.to_string()).unwrap_or_default();
        let mut m = std::collections::HashMap::new();
        m.insert("__dbquery__".to_string(), Value::Bool(true));
        m.insert("collection".to_string(), Value::make_str(collection));
        m.insert("filters".to_string(), Value::Null);
        m.insert("sort_field".to_string(), Value::Null);
        m.insert("sort_dir".to_string(), Value::Null);
        m.insert("limit_n".to_string(), Value::Null);
        return Ok(Some(Value::make_map(m)));
    }
    // Feature 12 (new): audit(action, details) — audit trail
    if name == "audit" {
        let action = args.first().map(|v| v.to_string()).unwrap_or_default();
        let details = args.get(1).cloned().unwrap_or(Value::Null);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let key = format!("_audit_{}_{}", now, uuid::Uuid::new_v4());
        let mut entry = std::collections::HashMap::new();
        entry.insert("action".to_string(), Value::make_str(action));
        entry.insert("details".to_string(), details);
        entry.insert("timestamp".to_string(), Value::Int(now));
        let val = Value::make_map(entry);
        shared.lock().await.db.set("_audit", &key, val);
        return Ok(Some(Value::Null));
    }
    if name == "audit_log" {
        let n = args.first().and_then(|v| v.as_int()).unwrap_or(10) as usize;
        let st = shared.lock().await;
        let mut entries: Vec<(String, Value)> = st.db.all("_audit");
        // Sort by key (which contains timestamp)
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        entries.truncate(n);
        let list: Vec<Value> = entries.into_iter().map(|(_, v)| v).collect();
        return Ok(Some(Value::make_list(list)));
    }
    // Feature 9: Chat action builtins (typing, pin_msg, unpin_msg, mute_user)
    // These are handled in exec with outputs, not here
    // Feature 5: expect(val) — returns a special expect wrapper
    if name == "expect" {
        let val = args.first().cloned().unwrap_or(Value::Null);
        let mut m = std::collections::HashMap::new();
        m.insert("__expect__".to_string(), val);
        return Ok(Some(Value::make_map(m)));
    }
    // Feature 7: remember / recall / forget / memories
    if name == "remember" {
        if args.len() < 3 { return Err(crate::runtime_err!("remember(user_id, key, value)")); }
        let user_id = args[0].to_string();
        let key = args[1].to_string();
        let val = args[2].clone();
        let db_key = format!("_memory_{user_id}_{key}");
        shared.lock().await.db.set("_memories", &db_key, val);
        return Ok(Some(Value::Null));
    }
    if name == "recall" {
        if args.len() < 2 { return Err(crate::runtime_err!("recall(user_id, key)")); }
        let user_id = args[0].to_string();
        let key = args[1].to_string();
        let db_key = format!("_memory_{user_id}_{key}");
        let val = shared.lock().await.db.get("_memories", &db_key);
        return Ok(Some(val));
    }
    if name == "forget" {
        if args.len() < 2 { return Err(crate::runtime_err!("forget(user_id, key)")); }
        let user_id = args[0].to_string();
        let key = args[1].to_string();
        let db_key = format!("_memory_{user_id}_{key}");
        shared.lock().await.db.del("_memories", &db_key);
        return Ok(Some(Value::Null));
    }
    if name == "memories" {
        if args.is_empty() { return Err(crate::runtime_err!("memories(user_id)")); }
        let user_id = args[0].to_string();
        let prefix = format!("_memory_{user_id}_");
        let st = shared.lock().await;
        let all = st.db.all("_memories");
        let mut result = std::collections::HashMap::new();
        for (k, v) in all {
            if let Some(key) = k.strip_prefix(&prefix) {
                result.insert(key.to_string(), v);
            }
        }
        return Ok(Some(Value::make_map(result)));
    }
    // Feature N2: extract(text) — entity extraction
    if name == "extract" {
        let text = args.first().map(|v| v.to_string()).unwrap_or_default();
        let st = shared.lock().await;
        let mut result = std::collections::HashMap::new();
        for edef in &st.entity_defs {
            let ename = &edef.name;
            match &edef.kind {
                crate::ast::EntityKind::Builtin => {
                    // Builtin patterns: email, phone, url, number
                    let patterns: &[(&str, &str)] = &[
                        ("email", r"[\w.+-]+@[\w-]+\.[\w.-]+"),
                        ("phone", r"\+?\d[\d\s\-]{6,14}\d"),
                        ("url", r"https?://\S+"),
                        ("number", r"\b\d+(?:\.\d+)?\b"),
                    ];
                    for (pname, pat) in patterns {
                        if ename == pname {
                            if let Ok(re) = regex::Regex::new(pat) {
                                let matches: Vec<Value> = re.find_iter(&text)
                                    .map(|m| Value::make_str(m.as_str()))
                                    .collect();
                                if !matches.is_empty() {
                                    result.insert(ename.clone(), Value::make_list(matches));
                                }
                            }
                        }
                    }
                }
                crate::ast::EntityKind::List(values) => {
                    let text_lower = text.to_lowercase();
                    let matches: Vec<Value> = values.iter()
                        .filter(|v| text_lower.contains(&v.to_lowercase()))
                        .map(|v| Value::make_str(v.as_str()))
                        .collect();
                    if !matches.is_empty() {
                        result.insert(ename.clone(), Value::make_list(matches));
                    }
                }
            }
        }
        return Ok(Some(Value::make_map(result)));
    }
    // Feature N4: track(event, data) — analytics tracking
    if name == "track" {
        let event_name = args.first().map(|v| v.to_string()).unwrap_or_default();
        let data = args.get(1).cloned().unwrap_or(Value::Null);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let mut st = shared.lock().await;
        st.analytics.push(crate::interpreter::AnalyticsEvent {
            name: event_name,
            data,
            timestamp: now,
        });
        return Ok(Some(Value::Null));
    }
    // Feature N4: analytics.funnel(events, period_ms)
    if name == "analytics_funnel" {
        let events = args.first().cloned().unwrap_or(Value::Null);
        let period_ms = args.get(1).and_then(|v| v.as_int()).unwrap_or(86_400_000) as u64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let cutoff = now.saturating_sub(period_ms);
        let event_names: Vec<String> = match &events {
            Value::List(l) => l.borrow().iter().map(|v| v.to_string()).collect(),
            _ => vec![],
        };
        let st = shared.lock().await;
        let mut counts = std::collections::HashMap::new();
        for ev in &st.analytics {
            if ev.timestamp >= cutoff && event_names.contains(&ev.name) {
                *counts.entry(ev.name.clone()).or_insert(0i64) += 1;
            }
        }
        let result: std::collections::HashMap<String, Value> = event_names.iter()
            .map(|n| (n.clone(), Value::Int(*counts.get(n).unwrap_or(&0))))
            .collect();
        return Ok(Some(Value::make_map(result)));
    }
    // Feature W7: RBAC builtins — assign_role, check_permission, get_role
    if name == "assign_role" {
        if args.len() < 2 { return Err(crate::runtime_err!("assign_role(user_id, role_name)")); }
        let user_id = args[0].as_int().unwrap_or(0);
        let role = args[1].to_string();
        shared.lock().await.user_roles.insert(user_id, role);
        return Ok(Some(Value::Null));
    }
    if name == "check_permission" {
        if args.len() < 2 { return Err(crate::runtime_err!("check_permission(user_id, perm_name)")); }
        let user_id = args[0].as_int().unwrap_or(0);
        let perm = args[1].to_string();
        let st = shared.lock().await;
        let role = st.user_roles.get(&user_id).cloned().unwrap_or(st.rbac_default_role.clone());
        let has = st.rbac_roles.get(&role).map(|perms| perms.contains(&perm)).unwrap_or(false);
        return Ok(Some(Value::Bool(has)));
    }
    if name == "get_role" {
        if args.is_empty() { return Err(crate::runtime_err!("get_role(user_id)")); }
        let user_id = args[0].as_int().unwrap_or(0);
        let st = shared.lock().await;
        let role = st.user_roles.get(&user_id).cloned().unwrap_or(st.rbac_default_role.clone());
        return Ok(Some(Value::make_str(role)));
    }
    // Feature W11: validate(value, type_name) — type validation
    if name == "validate_type" {
        if args.len() < 2 { return Err(crate::runtime_err!("validate_type(value, type_name)")); }
        let val = &args[0];
        let type_name = args[1].to_string();
        let st = shared.lock().await;
        if let Some(td) = st.type_defs.get(&type_name) {
            let base_valid = match td.base_type.as_str() {
                "int"   => val.as_int().is_some(),
                "float" => val.as_float().is_some(),
                "str"   => val.as_str().is_some(),
                "bool"  => matches!(val, Value::Bool(_)),
                _       => true,
            };
            return Ok(Some(Value::Bool(base_valid)));
        }
        return Ok(Some(Value::Bool(true)));
    }
    // Feature N6: channel(name) — returns a channel sender/receiver map
    if name == "channel" {
        let ch_name = args.first().map(|v| v.to_string()).unwrap_or_default();
        let mut st = shared.lock().await;
        st.channels.entry(ch_name.clone()).or_default();
        let mut m = std::collections::HashMap::new();
        m.insert("__channel__".to_string(), Value::make_str(ch_name));
        return Ok(Some(Value::make_map(m)));
    }
    // Feature N6: channel_send(name, value)
    if name == "channel_send" {
        let ch_name = args.first().map(|v| v.to_string()).unwrap_or_default();
        let val = args.get(1).cloned().unwrap_or(Value::Null);
        let mut st = shared.lock().await;
        st.channels.entry(ch_name).or_default().push_back(val);
        return Ok(Some(Value::Null));
    }
    // Feature N6: channel_recv(name)
    if name == "channel_recv" {
        let ch_name = args.first().map(|v| v.to_string()).unwrap_or_default();
        let mut st = shared.lock().await;
        let val = st.channels.entry(ch_name).or_default().pop_front().unwrap_or(Value::Null);
        return Ok(Some(val));
    }
    // Feature 12: i18n function — handled via shared state
    if name == "i18n" {
        let key = args.first().map(|v| v.to_string()).unwrap_or_default();
        let lang = args.get(1).map(|v| v.to_string());
        let st = shared.lock().await;
        let default_lang = st.default_lang.clone();
        let lang_code = lang.unwrap_or(default_lang);
        let val = st.i18n_strings.get(&lang_code)
            .and_then(|kv| kv.get(&key))
            .cloned()
            .or_else(|| {
                // Fallback to "en"
                st.i18n_strings.get("en")
                    .and_then(|kv| kv.get(&key))
                    .cloned()
            })
            .unwrap_or_else(|| Value::make_str(format!("[missing:{key}]")));
        return Ok(Some(val));
    }
    Ok(None)
}
