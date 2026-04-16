/// Feature 8: Utility builtins — uuid(), sha256(), base64_encode(), base64_decode(), sleep(ms)

use crate::error::{GravError, GravResult};
use crate::value::Value;

pub async fn call_utils_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    match name {
        "uuid" => {
            let id = uuid::Uuid::new_v4().to_string();
            Ok(Some(Value::make_str(id)))
        }
        "sha256" => {
            use sha2::{Sha256, Digest};
            let data = args.first().map(|v| v.to_string()).unwrap_or_default();
            let mut hasher = Sha256::new();
            hasher.update(data.as_bytes());
            let result = hasher.finalize();
            let hex_str = hex::encode(result);
            Ok(Some(Value::make_str(hex_str)))
        }
        "base64_encode" => {
            use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
            let data = args.first().map(|v| v.to_string()).unwrap_or_default();
            let encoded = B64.encode(data.as_bytes());
            Ok(Some(Value::make_str(encoded)))
        }
        "base64_decode" => {
            use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
            let data = args.first().map(|v| v.to_string()).unwrap_or_default();
            let decoded = B64.decode(data.as_bytes())
                .map_err(|e| GravError::Runtime(format!("base64_decode: {e}")))?;
            let s = String::from_utf8(decoded)
                .map_err(|e| GravError::Runtime(format!("base64_decode: utf8 error: {e}")))?;
            Ok(Some(Value::make_str(s)))
        }
        "sleep" => {
            let ms = args.first()
                .and_then(|v| v.as_int())
                .unwrap_or(0) as u64;
            tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
            Ok(Some(Value::Null))
        }
        _ => Ok(None),
    }
}
