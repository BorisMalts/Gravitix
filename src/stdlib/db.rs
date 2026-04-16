//! Persistent key-value database backed by a JSON file.
//! db_set(table, key, value)  → null
//! db_get(table, key)         → value | null
//! db_del(table, key)         → null
//! db_has(table, key)         → bool
//! db_keys(table)             → list<str>
//! db_query(table)            → list<{key, value}> (all entries)
//! db_count(table)            → int

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::GravResult;
use crate::value::Value;
use crate::runtime_err;

/// In-memory database with persistence to a JSON file.
#[derive(Default)]
pub struct Db {
    /// table_name -> (key -> value_json)
    tables: HashMap<String, HashMap<String, serde_json::Value>>,
    path:   Option<std::path::PathBuf>,
}

impl Db {
    pub fn new(path: Option<std::path::PathBuf>) -> Self {
        let mut db = Db { tables: HashMap::new(), path: path.clone() };
        if let Some(p) = &path {
            if p.exists() {
                if let Ok(data) = std::fs::read_to_string(p) {
                    if let Ok(tables) = serde_json::from_str(&data) {
                        db.tables = tables;
                    }
                }
            }
        }
        db
    }

    pub fn save(&self) {
        if let Some(p) = &self.path {
            if let Ok(json) = serde_json::to_string(&self.tables) {
                let _ = std::fs::write(p, json);
            }
        }
    }

    pub fn get(&self, table: &str, key: &str) -> Value {
        self.tables.get(table)
            .and_then(|t| t.get(key))
            .map(json_to_value)
            .unwrap_or(Value::Null)
    }

    pub fn set(&mut self, table: &str, key: &str, val: Value) {
        let t = self.tables.entry(table.to_string()).or_default();
        t.insert(key.to_string(), value_to_json(&val));
        self.save();
    }

    pub fn del(&mut self, table: &str, key: &str) -> bool {
        let removed = self.tables.get_mut(table).map_or(false, |t| t.remove(key).is_some());
        if removed { self.save(); }
        removed
    }

    pub fn has(&self, table: &str, key: &str) -> bool {
        self.tables.get(table).map_or(false, |t| t.contains_key(key))
    }

    pub fn keys(&self, table: &str) -> Vec<String> {
        self.tables.get(table).map_or_else(Vec::new, |t| t.keys().cloned().collect())
    }

    pub fn all(&self, table: &str) -> Vec<(String, Value)> {
        self.tables.get(table).map_or_else(Vec::new, |t| {
            t.iter().map(|(k, v)| (k.clone(), json_to_value(v))).collect()
        })
    }

    pub fn count(&self, table: &str) -> usize {
        self.tables.get(table).map_or(0, |t| t.len())
    }
}

fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Value::Int(i) }
            else if let Some(f) = n.as_f64() { Value::Float(f) }
            else { Value::Null }
        }
        serde_json::Value::String(s) => Value::make_str(s.clone()),
        serde_json::Value::Array(a) => Value::make_list(a.iter().map(json_to_value).collect()),
        serde_json::Value::Object(o) => {
            let map: HashMap<String, Value> = o.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::make_map(map)
        }
    }
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null        => serde_json::Value::Null,
        Value::Bool(b)     => serde_json::Value::Bool(*b),
        Value::Int(n)      => serde_json::json!(*n),
        Value::Float(f)    => serde_json::json!(*f),
        Value::Str(s)      => serde_json::Value::String(s.as_ref().clone()),
        Value::List(l)     => serde_json::Value::Array(l.borrow().iter().map(value_to_json).collect()),
        Value::Map(m)      => {
            let obj: serde_json::Map<String, serde_json::Value> = m.borrow().iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::Null,
    }
}

/// Call a db_* function. Returns None if function name not recognized.
pub async fn call_db(
    name: &str,
    args: &[Value],
    shared: &Arc<Mutex<crate::interpreter::SharedState>>,
) -> GravResult<Option<Value>> {
    match name {
        "db_set" | "db.set" => {
            if args.len() < 3 { return Err(runtime_err!("db_set(table, key, value)")); }
            let table = args[0].to_string();
            let key   = args[1].to_string();
            let val   = args[2].clone();
            shared.lock().await.db.set(&table, &key, val);
            Ok(Some(Value::Null))
        }
        "db_get" | "db.get" => {
            if args.len() < 2 { return Err(runtime_err!("db_get(table, key)")); }
            let table = args[0].to_string();
            let key   = args[1].to_string();
            Ok(Some(shared.lock().await.db.get(&table, &key)))
        }
        "db_del" | "db_delete" | "db.del" | "db.delete" => {
            if args.len() < 2 { return Err(runtime_err!("db_del(table, key)")); }
            let table = args[0].to_string();
            let key   = args[1].to_string();
            let removed = shared.lock().await.db.del(&table, &key);
            Ok(Some(Value::Bool(removed)))
        }
        "db_has" | "db.has" => {
            if args.len() < 2 { return Err(runtime_err!("db_has(table, key)")); }
            let table = args[0].to_string();
            let key   = args[1].to_string();
            Ok(Some(Value::Bool(shared.lock().await.db.has(&table, &key))))
        }
        "db_keys" | "db.keys" => {
            if args.is_empty() { return Err(runtime_err!("db_keys(table)")); }
            let table = args[0].to_string();
            let keys = shared.lock().await.db.keys(&table);
            Ok(Some(Value::make_list(keys.into_iter().map(Value::make_str).collect())))
        }
        "db_query" | "db.query" | "db_all" | "db.all" => {
            if args.is_empty() { return Err(runtime_err!("db_query(table)")); }
            let table = args[0].to_string();
            let rows = shared.lock().await.db.all(&table);
            let list: Vec<Value> = rows.into_iter().map(|(k, v)| {
                let mut m = HashMap::new();
                m.insert("key".to_string(), Value::make_str(k));
                m.insert("value".to_string(), v);
                Value::make_map(m)
            }).collect();
            Ok(Some(Value::make_list(list)))
        }
        "db_count" | "db.count" => {
            if args.is_empty() { return Err(runtime_err!("db_count(table)")); }
            let table = args[0].to_string();
            Ok(Some(Value::Int(shared.lock().await.db.count(&table) as i64)))
        }
        _ => Ok(None),
    }
}
