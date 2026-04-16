use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

pub fn call_collections_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        "len" => {
            match args.first() {
                Some(Value::Str(s))    => Value::Int(s.len() as i64),
                Some(Value::List(l))   => Value::Int(l.borrow().len() as i64),
                Some(Value::Map(m))    => Value::Int(m.borrow().len() as i64),
                Some(v) => return Err(runtime_err!("len: cannot get length of {}", v.type_name())),
                None    => return Err(runtime_err!("len: expected 1 argument")),
            }
        }
        "range" => {
            let start = args.first().and_then(|v| v.as_int()).unwrap_or(0);
            let end   = args.get(1).and_then(|v| v.as_int()).unwrap_or(10);
            let step  = args.get(2).and_then(|v| v.as_int()).unwrap_or(1);
            if step == 0 { return Err(runtime_err!("range: step cannot be 0")); }
            let mut v = Vec::new();
            let mut i = start;
            while if step > 0 { i < end } else { i > end } {
                v.push(Value::Int(i));
                i += step;
            }
            Value::make_list(v)
        }
        "push" => {
            if let Some(Value::List(l)) = args.first() {
                for v in &args[1..] { l.borrow_mut().push(v.clone()); }
                Value::Null
            } else { return Err(runtime_err!("push: expected list as first arg")); }
        }
        "pop" => {
            if let Some(Value::List(l)) = args.first() {
                l.borrow_mut().pop().unwrap_or(Value::Null)
            } else { return Err(runtime_err!("pop: expected list")); }
        }
        "reverse" => {
            if let Some(Value::List(l)) = args.first() {
                let rev: Vec<Value> = l.borrow().iter().cloned().rev().collect();
                Value::make_list(rev)
            } else { return Err(runtime_err!("reverse: expected list")); }
        }
        "map_list" => {
            // map_list(list, fn_name) — applies fn to each element
            return Err(runtime_err!("map_list: use for loops for list transformation"));
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}
