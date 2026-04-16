use std::sync::Arc;
use tokio::sync::Mutex;

use crate::interpreter::SharedState;
use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

pub async fn call_state_builtin(
    name:   &str,
    args:   &[Value],
    shared: &Arc<Mutex<SharedState>>,
) -> GravResult<Option<Value>> {
    let v = match name {
        "state_get" => {
            let key = get_str(args, 0, "state_get")?;
            let st  = shared.lock().await;
            st.bot_state.get(&key).cloned().unwrap_or(Value::Null)
        }
        "state_set" => {
            let key = get_str(args, 0, "state_set")?;
            let val = args.get(1).cloned().unwrap_or(Value::Null);
            shared.lock().await.bot_state.insert(key, val);
            Value::Null
        }
        "state_del" => {
            let key = get_str(args, 0, "state_del")?;
            shared.lock().await.bot_state.remove(&key);
            Value::Null
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn get_str(args: &[Value], idx: usize, fn_name: &str) -> GravResult<String> {
    match args.get(idx) {
        Some(v) => Ok(v.to_string()),
        None    => Err(runtime_err!("{fn_name}: missing string argument at position {idx}")),
    }
}
