use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

pub fn call_convert_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        "int" | "parse_int" => {
            let s = args.first().map(|v| v.to_string()).unwrap_or_default();
            Value::Int(s.trim().parse::<i64>()
                .map_err(|_| runtime_err!("cannot convert '{}' to int", s))?)
        }
        "float" | "parse_float" => {
            let s = args.first().map(|v| v.to_string()).unwrap_or_default();
            Value::Float(s.trim().parse::<f64>()
                .map_err(|_| runtime_err!("cannot convert '{}' to float", s))?)
        }
        "str" | "to_str" => {
            Value::make_str(args.first().map(|v| v.to_string()).unwrap_or_default())
        }
        "bool" => {
            Value::Bool(args.first().map(|v| v.is_truthy()).unwrap_or(false))
        }
        "type_of" => {
            Value::make_str(args.first().map(|v| v.type_name()).unwrap_or("null"))
        }
        "is_null"   => Value::Bool(matches!(args.first(), Some(Value::Null) | None)),
        "is_int"    => Value::Bool(matches!(args.first(), Some(Value::Int(_)))),
        "is_float"  => Value::Bool(matches!(args.first(), Some(Value::Float(_)))),
        "is_str"    => Value::Bool(matches!(args.first(), Some(Value::Str(_)))),
        "is_list"   => Value::Bool(matches!(args.first(), Some(Value::List(_)))),
        "is_map"    => Value::Bool(matches!(args.first(), Some(Value::Map(_)))),
        "is_bool"   => Value::Bool(matches!(args.first(), Some(Value::Bool(_)))),
        _ => return Ok(None),
    };
    Ok(Some(v))
}
