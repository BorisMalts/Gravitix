use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

pub fn call_string_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        "trim" => {
            let s = get_str(args, 0, "trim")?;
            Value::make_str(s.trim().to_string())
        }
        "lowercase" | "to_lower" => {
            let s = get_str(args, 0, "lowercase")?;
            Value::make_str(s.to_lowercase())
        }
        "uppercase" | "to_upper" => {
            let s = get_str(args, 0, "uppercase")?;
            Value::make_str(s.to_uppercase())
        }
        "split" => {
            let s   = get_str(args, 0, "split")?;
            let sep = get_str(args, 1, "split").unwrap_or_default();
            let sep = if sep.is_empty() { " ".to_string() } else { sep };
            Value::make_list(s.split(sep.as_str()).map(Value::make_str).collect())
        }
        "join" => {
            let list = match args.first() {
                Some(Value::List(l)) => l.borrow().iter().map(|v| v.to_string()).collect::<Vec<_>>(),
                _ => return Err(runtime_err!("join: expected list as first arg")),
            };
            let sep = get_str(args, 1, "join").unwrap_or_default();
            Value::make_str(list.join(&sep))
        }
        "replace" => {
            let s    = get_str(args, 0, "replace")?;
            let from = get_str(args, 1, "replace")?;
            let to   = get_str(args, 2, "replace").unwrap_or_default();
            Value::make_str(s.replace(from.as_str(), to.as_str()))
        }
        "sanitize" => {
            // Remove control characters / HTML special chars
            let s = get_str(args, 0, "sanitize")?;
            let clean: String = s.chars()
                .map(|c| match c { '<' => '[', '>' => ']', '&' => '+', _ => c })
                .filter(|c| !c.is_control())
                .collect();
            Value::make_str(clean)
        }
        "contains" => {
            match args.first() {
                Some(Value::Str(s)) => {
                    let needle = get_str(args, 1, "contains")?;
                    Value::Bool(s.contains(needle.as_str()))
                }
                Some(Value::List(l)) => {
                    let target = args.get(1).cloned().unwrap_or(Value::Null);
                    Value::Bool(l.borrow().iter().any(|v| v == &target))
                }
                _ => return Err(runtime_err!("contains: expected str or list")),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

pub(super) fn get_str(args: &[Value], idx: usize, fn_name: &str) -> GravResult<String> {
    match args.get(idx) {
        Some(v) => Ok(v.to_string()),
        None    => Err(crate::runtime_err!("{fn_name}: missing string argument at position {idx}")),
    }
}
