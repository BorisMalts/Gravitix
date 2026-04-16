use crate::value::Value;
use crate::error::GravResult;

pub fn call_io_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        "print" => {
            let s = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            println!("{s}");
            Value::Null
        }
        "log" => {
            let s = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            eprintln!("[gravitix] {s}");
            Value::Null
        }
        "fetch" => {
            // fetch(url) — SSRF check only (async fetch is done via vortex stdlib)
            let url_str = args.first()
                .map(|v| v.to_string())
                .unwrap_or_default();

            if is_ssrf_blocked(&url_str) {
                return Err(crate::runtime_err!("fetch: blocked URL (SSRF protection)"));
            }

            // Note: async fetch requires use of vortex_send or a dedicated async builtin.
            // Returning Null here as a placeholder — use async HTTP via tokio task externally.
            eprintln!("[gravitix] fetch: use vortex_send for HTTP requests in async context");
            Value::Null
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn is_ssrf_blocked(url: &str) -> bool {
    let blocked = [
        "localhost", "127.", "0.0.0.0",
        "10.", "192.168.", "172.16.", "172.17.", "172.18.", "172.19.",
        "172.20.", "172.21.", "172.22.", "172.23.", "172.24.", "172.25.",
        "172.26.", "172.27.", "172.28.", "172.29.", "172.30.", "172.31.",
        "169.254.", "::1", "[::1]", "fd", "fc",
    ];
    let url_lower = url.to_lowercase();
    blocked.iter().any(|b| url_lower.contains(b))
}
