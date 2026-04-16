/// Feature 7: Regex builtins — `regex_match`, `regex_replace`, `regex_find_all`, `regex_captures`

use crate::error::{GravError, GravResult};
use crate::value::Value;

pub fn call_regex_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    match name {
        "regex_match" => {
            let pattern = args.first().map(|v| v.to_string()).unwrap_or_default();
            let text = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            let re = regex::Regex::new(&pattern)
                .map_err(|e| GravError::Runtime(format!("regex_match: {e}")))?;
            Ok(Some(Value::Bool(re.is_match(&text))))
        }
        "regex_find_all" => {
            let pattern = args.first().map(|v| v.to_string()).unwrap_or_default();
            let text = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            let re = regex::Regex::new(&pattern)
                .map_err(|e| GravError::Runtime(format!("regex_find_all: {e}")))?;
            let matches: Vec<Value> = re.find_iter(&text)
                .map(|m| Value::make_str(m.as_str()))
                .collect();
            Ok(Some(Value::make_list(matches)))
        }
        "regex_replace" => {
            let pattern = args.first().map(|v| v.to_string()).unwrap_or_default();
            let text = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            let replacement = args.get(2).map(|v| v.to_string()).unwrap_or_default();
            let re = regex::Regex::new(&pattern)
                .map_err(|e| GravError::Runtime(format!("regex_replace: {e}")))?;
            let result = re.replace_all(&text, replacement.as_str()).to_string();
            Ok(Some(Value::make_str(result)))
        }
        "regex_captures" => {
            let pattern = args.first().map(|v| v.to_string()).unwrap_or_default();
            let text = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            let re = regex::Regex::new(&pattern)
                .map_err(|e| GravError::Runtime(format!("regex_captures: {e}")))?;
            match re.captures(&text) {
                Some(caps) => {
                    let groups: Vec<Value> = (0..caps.len())
                        .map(|i| caps.get(i)
                            .map(|m| Value::make_str(m.as_str()))
                            .unwrap_or(Value::Null))
                        .collect();
                    Ok(Some(Value::make_list(groups)))
                }
                None => Ok(Some(Value::Null)),
            }
        }
        _ => Ok(None),
    }
}
