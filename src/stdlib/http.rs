/// Feature 5: HTTP client — `http.get(url)`, `http.post(url, body)`, etc.

use std::collections::HashMap;
use crate::error::{GravError, GravResult};
use crate::value::Value;

async fn do_request(method: &str, args: &[Value]) -> GravResult<Value> {
    let url = args.first().map(|v| v.to_string()).unwrap_or_default();
    if url.is_empty() {
        return Err(GravError::Runtime(format!("http.{method}: url is required")));
    }

    // Parse optional options map (second arg)
    let opts = args.get(1);
    let mut req_headers: HashMap<String, String> = HashMap::new();
    let mut body_str: Option<String> = None;
    let mut timeout_ms: u64 = 10000;

    if let Some(Value::Map(m)) = opts {
        let m = m.borrow();
        if let Some(Value::Map(h)) = m.get("headers") {
            for (k, v) in h.borrow().iter() {
                req_headers.insert(k.clone(), v.to_string());
            }
        }
        if let Some(b) = m.get("body") {
            body_str = Some(b.to_string());
        }
        if let Some(t) = m.get("timeout") {
            timeout_ms = t.as_int().unwrap_or(10000) as u64;
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| GravError::Runtime(format!("http.{method}: client error: {e}")))?;

    let mut builder = match method {
        "get"    => client.get(&url),
        "post"   => client.post(&url),
        "put"    => client.put(&url),
        "delete" => client.delete(&url),
        _        => return Err(GravError::Runtime(format!("http: unknown method '{method}'"))),
    };

    for (k, v) in &req_headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if let Some(body) = body_str {
        builder = builder.body(body);
    }

    let resp = builder.send().await
        .map_err(|e| GravError::Runtime(format!("http.{method}: request failed: {e}")))?;

    let status = resp.status().as_u16() as i64;
    let ok = resp.status().is_success();

    let resp_headers: HashMap<String, Value> = resp.headers().iter()
        .map(|(k, v)| (k.as_str().to_string(), Value::make_str(v.to_str().unwrap_or(""))))
        .collect();

    let body_text = resp.text().await
        .map_err(|e| GravError::Runtime(format!("http.{method}: read body: {e}")))?;

    let mut result = HashMap::new();
    result.insert("status".to_string(), Value::Int(status));
    result.insert("body".to_string(), Value::make_str(body_text));
    result.insert("headers".to_string(), Value::make_map(resp_headers));
    result.insert("ok".to_string(), Value::Bool(ok));

    Ok(Value::make_map(result))
}

pub async fn call_http_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    match name {
        "http_get"    => Ok(Some(do_request("get", args).await?)),
        "http_post"   => Ok(Some(do_request("post", args).await?)),
        "http_put"    => Ok(Some(do_request("put", args).await?)),
        "http_delete" => Ok(Some(do_request("delete", args).await?)),
        _ => Ok(None),
    }
}
