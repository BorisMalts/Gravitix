/// Feature 6: JSON builtins — `json_parse(str)`, `json_stringify(val)`

use std::collections::HashMap;
use crate::error::{GravError, GravResult};
use crate::value::Value;

fn json_to_value(j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::make_str(s.as_str()),
        serde_json::Value::Array(arr) => {
            let v: Vec<Value> = arr.iter().map(json_to_value).collect();
            Value::make_list(v)
        }
        serde_json::Value::Object(obj) => {
            let m: HashMap<String, Value> = obj.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::make_map(m)
        }
    }
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(n) => serde_json::json!(*n),
        Value::Float(f) => serde_json::json!(*f),
        Value::Str(s) => serde_json::Value::String(s.as_ref().clone()),
        Value::List(l) => {
            let arr: Vec<serde_json::Value> = l.borrow().iter().map(value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m.borrow().iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Complex(re, im) => {
            let s = if *im >= 0.0 { format!("{re}+{im}i") } else { format!("{re}{im}i") };
            serde_json::Value::String(s)
        }
        Value::Fn(_) => serde_json::Value::String("<fn>".to_string()),
        Value::Ctx(_) => serde_json::Value::String("<ctx>".to_string()),
    }
}

pub fn call_json_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    match name {
        "json_parse" => {
            let s = args.first().map(|v| v.to_string()).unwrap_or_default();
            let parsed: serde_json::Value = serde_json::from_str(&s)
                .map_err(|e| GravError::Runtime(format!("json_parse: {e}")))?;
            Ok(Some(json_to_value(&parsed)))
        }
        "json_stringify" => {
            let val = args.first().cloned().unwrap_or(Value::Null);
            let j = value_to_json(&val);
            let s = serde_json::to_string(&j)
                .map_err(|e| GravError::Runtime(format!("json_stringify: {e}")))?;
            Ok(Some(Value::make_str(s)))
        }
        _ => Ok(None),
    }
}
