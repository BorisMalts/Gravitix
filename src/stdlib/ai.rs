/// Feature 1: `ai(prompt)` / `ai_chat(history, msg)` — LLM builtins
///
/// Uses Ollama or OpenAI-compatible API.
/// Env vars: OLLAMA_URL / OPENAI_URL, AI_MODEL (default: llama3)

use crate::error::{GravError, GravResult};
use crate::value::Value;

fn get_base_url() -> String {
    std::env::var("OPENAI_URL")
        .or_else(|_| std::env::var("OLLAMA_URL"))
        .unwrap_or_else(|_| "http://localhost:11434".to_string())
}

fn get_model() -> String {
    std::env::var("AI_MODEL").unwrap_or_else(|_| "llama3".to_string())
}

/// POST to Ollama /api/generate and return the response text.
pub async fn ai_call(prompt: &str) -> GravResult<String> {
    let base_url = get_base_url();
    let model    = get_model();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| GravError::Runtime(format!("ai: http client: {e}")))?;

    // Try OpenAI-compatible chat completions first if OPENAI_URL is set
    if std::env::var("OPENAI_URL").is_ok() {
        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "stream": false
        });
        let resp = client.post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send().await
            .map_err(|e| GravError::Runtime(format!("ai: request failed: {e}")))?;
        let json: serde_json::Value = resp.json().await
            .map_err(|e| GravError::Runtime(format!("ai: parse response: {e}")))?;
        let text = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        return Ok(text);
    }

    // Ollama /api/generate
    let url = format!("{}/api/generate", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false
    });
    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send().await
        .map_err(|e| GravError::Runtime(format!("ai: request failed: {e}")))?;
    let json: serde_json::Value = resp.json().await
        .map_err(|e| GravError::Runtime(format!("ai: parse response: {e}")))?;
    let text = json["response"].as_str().unwrap_or("").to_string();
    Ok(text)
}

/// POST to /v1/chat/completions with a history of messages + current message.
pub async fn ai_chat_call(history: &[Value], msg: &str) -> GravResult<String> {
    let base_url = get_base_url();
    let model    = get_model();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| GravError::Runtime(format!("ai_chat: http client: {e}")))?;

    // Build messages array from history
    let mut messages: Vec<serde_json::Value> = history.iter().filter_map(|v| {
        if let Value::Map(m) = v {
            let m = m.borrow();
            let text = m.get("text").map(|v| v.to_string()).unwrap_or_default();
            Some(serde_json::json!({"role": "user", "content": text}))
        } else {
            None
        }
    }).collect();
    messages.push(serde_json::json!({"role": "user", "content": msg}));

    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false
    });
    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send().await
        .map_err(|e| GravError::Runtime(format!("ai_chat: request failed: {e}")))?;
    let json: serde_json::Value = resp.json().await
        .map_err(|e| GravError::Runtime(format!("ai_chat: parse response: {e}")))?;
    let text = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    Ok(text)
}

/// Dispatch `ai` and `ai_chat` builtins.
pub async fn call_ai_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    match name {
        "ai" => {
            let prompt = args.first().map(|v| v.to_string()).unwrap_or_default();
            let result = ai_call(&prompt).await?;
            Ok(Some(Value::make_str(result)))
        }
        "ai_chat" => {
            let history: Vec<Value> = if let Some(Value::List(l)) = args.first() {
                l.borrow().clone()
            } else {
                vec![]
            };
            let msg = args.get(1).map(|v| v.to_string()).unwrap_or_default();
            let result = ai_chat_call(&history, &msg).await?;
            Ok(Some(Value::make_str(result)))
        }
        _ => Ok(None),
    }
}
