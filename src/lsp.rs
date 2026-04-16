/// Language Server Protocol server for Gravitix (.grav files) — v2.
///
/// Protocol : JSON-RPC 2.0 over stdin/stdout with Content-Length headers.
///
/// Implements:
///   initialize / initialized / shutdown / exit
///   textDocument/didOpen, didChange, didClose
///   textDocument/completion          — snippets + rich completions
///   textDocument/hover               — rich markdown docs
///   textDocument/signatureHelp       — parameter hints
///   textDocument/definition          — go-to-definition
///   textDocument/documentSymbol      — outline view
///   textDocument/formatting          — full document format
///   textDocument/codeLens            — run-test + fn lenses
///   textDocument/inlayHint           — type hints for let bindings
///   textDocument/semanticTokens/full — syntax highlighting
///   textDocument/publishDiagnostics  — parse errors
///   workspace/symbol                 — workspace symbol search

use std::io::{self, BufRead, Write};
use std::collections::HashMap;
use serde_json::{json, Value as JVal};

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn run_lsp() {
    let stdin  = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    let mut docs: HashMap<String, String> = HashMap::new();
    let mut reader = stdin.lock();

    loop {
        // Read Content-Length header
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 { break; }
        let header = header.trim();
        if !header.starts_with("Content-Length:") { continue; }
        let len: usize = header["Content-Length:".len()..].trim().parse().unwrap_or(0);

        // Consume blank line after header
        let mut blank = String::new();
        reader.read_line(&mut blank).ok();

        // Read body
        let mut body = vec![0u8; len];
        use std::io::Read;
        if reader.read_exact(&mut body).is_err() { break; }
        let body = match String::from_utf8(body) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let msg: JVal = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let method = msg["method"].as_str().unwrap_or("");
        let id     = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(JVal::Null);

        match method {
            // ── Lifecycle ─────────────────────────────────────────────────
            "initialize" => {
                let response = make_response(id, declare_capabilities());
                send(&mut out, &response);
            }

            "initialized" | "$/cancelRequest" | "$/setTrace" => {}

            "shutdown" => {
                send(&mut out, &make_response(id, JVal::Null));
            }

            "exit" => break,

            // ── Document sync ─────────────────────────────────────────────
            "textDocument/didOpen" => {
                if let (Some(uri), Some(text)) = (
                    params["textDocument"]["uri"].as_str(),
                    params["textDocument"]["text"].as_str(),
                ) {
                    let uri  = uri.to_string();
                    let text = text.to_string();
                    let diags = compute_diagnostics(&text);
                    docs.insert(uri.clone(), text);
                    publish_diagnostics(&mut out, &uri, diags);
                }
            }

            "textDocument/didChange" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    if let Some(change) = params["contentChanges"].as_array().and_then(|a| a.last()) {
                        if let Some(text) = change["text"].as_str() {
                            let uri  = uri.to_string();
                            let text = text.to_string();
                            let diags = compute_diagnostics(&text);
                            docs.insert(uri.clone(), text);
                            publish_diagnostics(&mut out, &uri, diags);
                        }
                    }
                }
            }

            "textDocument/didClose" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    docs.remove(uri);
                }
            }

            // ── Completion ────────────────────────────────────────────────
            "textDocument/completion" => {
                let items = completion_items();
                let response = make_response(id, json!({ "isIncomplete": false, "items": items }));
                send(&mut out, &response);
            }

            // ── Hover ─────────────────────────────────────────────────────
            "textDocument/hover" => {
                let text = doc_text(&docs, &params);
                let line = params["position"]["line"].as_u64().unwrap_or(0) as usize;
                let col  = params["position"]["character"].as_u64().unwrap_or(0) as usize;
                let word = word_at(&text, line, col);
                let response = if let Some(md) = hover_for(&word) {
                    make_response(id, json!({
                        "contents": { "kind": "markdown", "value": md }
                    }))
                } else {
                    make_response(id, JVal::Null)
                };
                send(&mut out, &response);
            }

            // ── Signature help ────────────────────────────────────────────
            "textDocument/signatureHelp" => {
                let text = doc_text(&docs, &params);
                let line = params["position"]["line"].as_u64().unwrap_or(0) as usize;
                let col  = params["position"]["character"].as_u64().unwrap_or(0) as usize;
                let response = match signature_help(&text, line, col) {
                    Some(sh) => make_response(id, sh),
                    None     => make_response(id, JVal::Null),
                };
                send(&mut out, &response);
            }

            // ── Go to definition ──────────────────────────────────────────
            "textDocument/definition" => {
                let uri  = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
                let text = docs.get(&uri).cloned().unwrap_or_default();
                let line = params["position"]["line"].as_u64().unwrap_or(0) as usize;
                let col  = params["position"]["character"].as_u64().unwrap_or(0) as usize;
                let word = word_at(&text, line, col);
                let response = match find_definition(&text, &uri, &word) {
                    Some(loc) => make_response(id, loc),
                    None      => make_response(id, JVal::Null),
                };
                send(&mut out, &response);
            }

            // ── Document symbols ──────────────────────────────────────────
            "textDocument/documentSymbol" => {
                let text = doc_text(&docs, &params);
                let symbols = document_symbols(&text);
                send(&mut out, &make_response(id, json!(symbols)));
            }

            // ── Formatting ────────────────────────────────────────────────
            "textDocument/formatting" => {
                let uri  = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
                let text = docs.get(&uri).cloned().unwrap_or_default();
                let response = match format_document(&text) {
                    Some(edit) => make_response(id, json!([edit])),
                    None       => make_response(id, json!([])),
                };
                send(&mut out, &response);
            }

            // ── Code lens ─────────────────────────────────────────────────
            "textDocument/codeLens" => {
                let text = doc_text(&docs, &params);
                let lenses = code_lenses(&text);
                send(&mut out, &make_response(id, json!(lenses)));
            }

            // ── Inlay hints ───────────────────────────────────────────────
            "textDocument/inlayHint" => {
                let text = doc_text(&docs, &params);
                let hints = inlay_hints(&text);
                send(&mut out, &make_response(id, json!(hints)));
            }

            // ── Semantic tokens ───────────────────────────────────────────
            "textDocument/semanticTokens/full" => {
                let text = doc_text(&docs, &params);
                let data = semantic_tokens(&text);
                send(&mut out, &make_response(id, json!({ "data": data })));
            }

            // ── Workspace symbols ─────────────────────────────────────────
            "workspace/symbol" => {
                let query = params["query"].as_str().unwrap_or("").to_string();
                // Search across all open documents
                let mut all_symbols: Vec<JVal> = Vec::new();
                for (uri, text) in &docs {
                    let syms = document_symbols_with_uri(text, uri);
                    for s in syms {
                        let name = s["name"].as_str().unwrap_or("").to_lowercase();
                        if query.is_empty() || name.contains(&query.to_lowercase()) {
                            all_symbols.push(s);
                        }
                    }
                }
                send(&mut out, &make_response(id, json!(all_symbols)));
            }

            _ => {
                // Unknown request — return error only for requests (have id), not notifications
                if let Some(id) = id {
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "Method not found" }
                    });
                    send(&mut out, &response);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Capabilities declaration
// ─────────────────────────────────────────────────────────────────────────────

fn declare_capabilities() -> JVal {
    json!({
        "capabilities": {
            "textDocumentSync": 1,
            "completionProvider": {
                "triggerCharacters": [".", " ", "("],
                "resolveProvider": false
            },
            "hoverProvider": true,
            "signatureHelpProvider": {
                "triggerCharacters": ["(", ","]
            },
            "definitionProvider": true,
            "documentSymbolProvider": true,
            "documentFormattingProvider": true,
            "codeLensProvider": { "resolveProvider": false },
            "inlayHintProvider": true,
            "semanticTokensProvider": {
                "legend": {
                    "tokenTypes": [
                        "keyword", "number", "string",
                        "function", "variable", "operator", "comment"
                    ],
                    "tokenModifiers": ["declaration", "definition"]
                },
                "full": true
            },
            "workspaceSymbolProvider": true
        },
        "serverInfo": {
            "name": "gravitix-lsp",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Diagnostics (syntax checking)
// ─────────────────────────────────────────────────────────────────────────────

fn compute_diagnostics(src: &str) -> Vec<JVal> {
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    let lex_result = Lexer::new(src).tokenize();
    let tokens = match lex_result {
        Ok(t) => t,
        Err(e) => return vec![error_diag(e.to_string(), 0, 0)],
    };
    match Parser::new(tokens).parse() {
        Ok(_) => vec![],
        Err(crate::error::GravError::Syntax { line, col, msg }) => {
            vec![error_diag(msg, (line.saturating_sub(1)) as u64, (col.saturating_sub(1)) as u64)]
        }
        Err(e) => vec![error_diag(e.to_string(), 0, 0)],
    }
}

fn error_diag(msg: String, line: u64, col: u64) -> JVal {
    json!({
        "range": {
            "start": { "line": line, "character": col },
            "end":   { "line": line, "character": col + 1 }
        },
        "severity": 1,
        "source": "gravitix",
        "message": msg
    })
}

fn publish_diagnostics(out: &mut impl Write, uri: &str, diags: Vec<JVal>) {
    let notif = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": { "uri": uri, "diagnostics": diags }
    });
    send(out, &notif);
}

// ─────────────────────────────────────────────────────────────────────────────
// Completions
// ─────────────────────────────────────────────────────────────────────────────

fn completion_items() -> Vec<JVal> {
    // insertTextFormat 1 = plain, 2 = snippet
    let mut items: Vec<JVal> = Vec::new();

    // ── Keyword snippets ─────────────────────────────────────────────────────
    let keyword_snippets: &[(&str, &str, &str)] = &[
        ("fn",       "fn ${1:name}(${2:params}) {\n\t$0\n}",                          "keyword"),
        ("on",       "on /${1:cmd} {\n\t$0\n}",                                        "keyword"),
        ("flow",     "flow ${1:name} {\n\t$0\n}",                                      "keyword"),
        ("state",    "state {\n\t${1:field}: ${2:int} = ${3:0}\n}",                    "keyword"),
        ("every",    "every ${1:5} ${2:min} {\n\t$0\n}",                               "keyword"),
        ("at",       "at \"${1:08:00}\" {\n\t$0\n}",                                   "keyword"),
        ("if",       "if ${1:cond} {\n\t$0\n}",                                        "keyword"),
        ("elif",     "elif ${1:cond} {\n\t$0\n}",                                      "keyword"),
        ("else",     "else {\n\t$0\n}",                                                "keyword"),
        ("while",    "while ${1:cond} {\n\t$0\n}",                                     "keyword"),
        ("for",      "for ${1:x} in ${2:list} {\n\t$0\n}",                             "keyword"),
        ("match",    "match ${1:expr} {\n\t${2:_} => $0\n}",                           "keyword"),
        ("try",      "try {\n\t$1\n} catch ${2:e} {\n\t$0\n}",                         "keyword"),
        ("let",      "let ${1:name} = $0",                                              "keyword"),
        ("return",   "return $0",                                                       "keyword"),
        ("emit",     "emit \"$0\"",                                                     "keyword"),
        ("keyboard", "keyboard \"${1:text}\", [[\"${2:label}\", \"${3:data}\"]]",       "keyword"),
        ("wait",     "wait msg",                                                        "keyword"),
        ("run",      "run flow ${1:name}",                                              "keyword"),
        ("test",     "test \"${1:test name}\" {\n\t$0\n}",                             "keyword"),
        ("struct",   "struct ${1:Name} {\n\t${2:field}: ${3:int}\n}",                  "keyword"),
        ("use",      "use \"${1:file.grav}\"",                                          "keyword"),
        ("callback", "on callback \"${1:data}\" {\n\t$0\n}",                           "keyword"),
        ("edit",     "edit ${1:msg_id} \"$0\"",                                         "keyword"),
        ("answer",   "answer \"$0\"",                                                   "keyword"),
        ("guard",    "guard ${1:cond}",                                                 "keyword"),
        ("break",    "break",                                                           "keyword"),
        ("continue", "continue",                                                        "keyword"),
        ("null",     "null",                                                            "keyword"),
        ("true",     "true",                                                            "keyword"),
        ("false",    "false",                                                           "keyword"),
    ];

    for (label, snippet, detail) in keyword_snippets {
        items.push(json!({
            "label": label,
            "kind": 14,
            "detail": detail,
            "insertText": snippet,
            "insertTextFormat": 2
        }));
    }

    // ── Constants ────────────────────────────────────────────────────────────
    let constants: &[(&str, &str)] = &[
        ("pi",           "3.14159265358979…"),
        ("e",            "2.71828182845904…"),
        ("tau",          "6.28318530717958… (2π)"),
        ("inf",          "Positive infinity"),
        ("phi",          "1.61803398874989… (golden ratio)"),
        ("euler_gamma",  "0.57721566490153… (Euler–Mascheroni constant)"),
    ];
    for (label, desc) in constants {
        items.push(json!({
            "label": label,
            "kind": 21,
            "detail": format!("const · float — {desc}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Core builtins ────────────────────────────────────────────────────────
    let builtins: &[(&str, &str, &str)] = &[
        ("len",         "len(v)",                              "builtin"),
        ("type_of",     "type_of(v) -> str",                  "builtin"),
        ("to_int",      "to_int(v) -> int",                   "builtin"),
        ("to_float",    "to_float(v) -> float",               "builtin"),
        ("to_str",      "to_str(v) -> str",                   "builtin"),
        ("print",       "print(v…)",                          "builtin"),
        ("log",         "log(v…)",                            "builtin"),
        ("random",      "random() -> float",                  "builtin"),
        ("rand_int",    "rand_int(min, max) -> int",          "builtin"),
        ("min",         "min(a, b)",                          "builtin"),
        ("max",         "max(a, b)",                          "builtin"),
        ("map_list",    "map_list(list, fn) -> list",         "builtin"),
        ("filter_list", "filter_list(list, fn) -> list",      "builtin"),
        ("fetch",       "fetch(url, method?, body?, headers?) -> any", "builtin"),
        ("json_parse",  "json_parse(str) -> any",             "builtin"),
        ("json_encode", "json_encode(val) -> str",            "builtin"),
        ("state_save",  "state_save()",                       "builtin"),
        ("state_load",  "state_load()",                       "builtin"),
        ("assert",      "assert(cond, msg?)",                 "builtin"),
        ("assert_eq",   "assert_eq(a, b, msg?)",              "builtin"),
        ("assert_ne",   "assert_ne(a, b, msg?)",              "builtin"),
        ("now_unix",    "now_unix() -> int",                  "builtin"),
        ("format_date", "format_date(ts, fmt?) -> str",       "builtin"),
        ("sleep_ms",    "sleep_ms(ms)",                       "builtin"),
    ];
    for (label, detail, cat) in builtins {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("{cat} · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Math functions ───────────────────────────────────────────────────────
    let math: &[(&str, &str)] = &[
        ("sin",     "sin(x: float) -> float"),
        ("cos",     "cos(x: float) -> float"),
        ("tan",     "tan(x: float) -> float"),
        ("asin",    "asin(x: float) -> float"),
        ("acos",    "acos(x: float) -> float"),
        ("atan",    "atan(x: float) -> float"),
        ("atan2",   "atan2(y, x: float) -> float"),
        ("sinh",    "sinh(x: float) -> float"),
        ("cosh",    "cosh(x: float) -> float"),
        ("tanh",    "tanh(x: float) -> float"),
        ("sqrt",    "sqrt(x: float) -> float"),
        ("cbrt",    "cbrt(x: float) -> float"),
        ("pow",     "pow(base, exp: float) -> float"),
        ("ln",      "ln(x: float) -> float"),
        ("log2",    "log2(x: float) -> float"),
        ("log10",   "log10(x: float) -> float"),
        ("exp",     "exp(x: float) -> float"),
        ("abs",     "abs(x) -> number"),
        ("floor",   "floor(x: float) -> float"),
        ("ceil",    "ceil(x: float) -> float"),
        ("round",   "round(x: float) -> float"),
        ("sign",    "sign(x: float) -> float"),
        ("trunc",   "trunc(x: float) -> float"),
        ("fract",   "fract(x: float) -> float"),
        ("hypot",   "hypot(x, y: float) -> float"),
        ("clamp",   "clamp(x, min, max: float) -> float"),
        ("lerp",    "lerp(a, b, t: float) -> float"),
        ("degrees", "degrees(x: float) -> float"),
        ("radians", "radians(x: float) -> float"),
    ];
    for (label, detail) in math {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("math · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Complex functions ────────────────────────────────────────────────────
    let complex: &[(&str, &str)] = &[
        ("complex", "complex(re, im: float) -> complex"),
        ("re",      "re(z: complex) -> float"),
        ("im",      "im(z: complex) -> float"),
        ("conj",    "conj(z: complex) -> complex"),
        ("cabs",    "cabs(z: complex) -> float"),
        ("arg",     "arg(z: complex) -> float"),
        ("polar",   "polar(r, theta: float) -> complex"),
        ("cexp",    "cexp(z: complex) -> complex"),
        ("clog",    "clog(z: complex) -> complex"),
        ("csin",    "csin(z: complex) -> complex"),
        ("ccos",    "ccos(z: complex) -> complex"),
        ("ctan",    "ctan(z: complex) -> complex"),
        ("cpow",    "cpow(z, w: complex) -> complex"),
        ("csqrt",   "csqrt(z: complex) -> complex"),
    ];
    for (label, detail) in complex {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("complex · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Special functions ────────────────────────────────────────────────────
    let special: &[(&str, &str)] = &[
        ("gamma",      "gamma(x: float) -> float"),
        ("lgamma",     "lgamma(x: float) -> float"),
        ("beta",       "beta(a, b: float) -> float"),
        ("erf",        "erf(x: float) -> float"),
        ("erfc",       "erfc(x: float) -> float"),
        ("zeta",       "zeta(s: float) -> float"),
        ("bessel_j",   "bessel_j(n, x: float) -> float"),
        ("bessel_y",   "bessel_y(n, x: float) -> float"),
        ("legendre",   "legendre(n: int, x: float) -> float"),
        ("hermite",    "hermite(n: int, x: float) -> float"),
        ("chebyshev",  "chebyshev(n: int, x: float) -> float"),
    ];
    for (label, detail) in special {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("special · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Linear algebra ───────────────────────────────────────────────────────
    let linalg: &[(&str, &str)] = &[
        ("dot",          "dot(a, b: list) -> float"),
        ("cross",        "cross(a, b: list) -> list"),
        ("norm",         "norm(v: list) -> float"),
        ("normalize",    "normalize(v: list) -> list"),
        ("det",          "det(m: list) -> float"),
        ("inv",          "inv(m: list) -> list"),
        ("transpose",    "transpose(m: list) -> list"),
        ("trace",        "trace(m: list) -> float"),
        ("mat_mul",      "mat_mul(a, b: list) -> list"),
        ("solve",        "solve(A, b: list) -> list"),
        ("eigenvalues",  "eigenvalues(m: list) -> list"),
    ];
    for (label, detail) in linalg {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("linalg · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Number theory ────────────────────────────────────────────────────────
    let numth: &[(&str, &str)] = &[
        ("gcd",       "gcd(a, b: int) -> int"),
        ("lcm",       "lcm(a, b: int) -> int"),
        ("factorial", "factorial(n: int) -> int"),
        ("is_prime",  "is_prime(n: int) -> bool"),
        ("primes",    "primes(n: int) -> list"),
        ("fib",       "fib(n: int) -> int"),
        ("binomial",  "binomial(n, k: int) -> int"),
        ("euler_phi", "euler_phi(n: int) -> int"),
    ];
    for (label, detail) in numth {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("number_theory · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Statistics ───────────────────────────────────────────────────────────
    let stats: &[(&str, &str)] = &[
        ("sum",         "sum(list) -> float"),
        ("avg",         "avg(list) -> float"),
        ("median",      "median(list) -> float"),
        ("variance",    "variance(list) -> float"),
        ("stddev",      "stddev(list) -> float"),
        ("cov",         "cov(a, b: list) -> float"),
        ("corr",        "corr(a, b: list) -> float"),
        ("normal_pdf",  "normal_pdf(x, mu, sigma: float) -> float"),
        ("normal_cdf",  "normal_cdf(x, mu, sigma: float) -> float"),
        ("linreg",      "linreg(xs, ys: list) -> [slope, intercept]"),
    ];
    for (label, detail) in stats {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("stats · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Transforms ───────────────────────────────────────────────────────────
    let transforms: &[(&str, &str)] = &[
        ("fft",       "fft(list) -> list"),
        ("ifft",      "ifft(list) -> list"),
        ("convolve",  "convolve(a, b: list) -> list"),
    ];
    for (label, detail) in transforms {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("transform · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    // ── Calculus ─────────────────────────────────────────────────────────────
    let calculus: &[(&str, &str)] = &[
        ("deriv",            "deriv(f, x: float) -> float"),
        ("integral_trapz",   "integral_trapz(f, a, b: float, n?: int) -> float"),
        ("integral_simpson", "integral_simpson(f, a, b: float, n?: int) -> float"),
        ("diff",             "diff(list) -> list"),
        ("cumsum",           "cumsum(list) -> list"),
        ("taylor_eval",      "taylor_eval(coeffs: list, x: float) -> float"),
    ];
    for (label, detail) in calculus {
        items.push(json!({
            "label": label,
            "kind": 3,
            "detail": format!("calculus · {detail}"),
            "insertText": label,
            "insertTextFormat": 1
        }));
    }

    items
}

// ─────────────────────────────────────────────────────────────────────────────
// Hover docs
// ─────────────────────────────────────────────────────────────────────────────

fn hover_for(word: &str) -> Option<String> {
    HOVER_MAP.iter()
        .find(|(k, _)| *k == word)
        .map(|(_, v)| v.to_string())
}

static HOVER_MAP: &[(&str, &str)] = &[
    // ── Language keywords ────────────────────────────────────────────────────
    ("fn", "\
## `fn` — Function definition

```gravitix
fn greet(name: str) -> str {
    return \"Hello, {name}!\"
}
```

**Parameters** are written as `name: type`. The return type is optional (`-> type`).
Functions are first-class values and can be passed as arguments."),

    ("on", "\
## `on` — Message / event handler

```gravitix
on /start {
    emit \"Welcome!\"
}
on msg {
    emit \"You said: {ctx.text}\"
}
on callback \"yes\" {
    emit \"Confirmed!\"
}
```

**Trigger kinds**

| Trigger | Fires when |
|---------|-----------|
| `/command` | User sends `/command` |
| `msg` | Any text message |
| `callback \"data\"` | Inline button with matching data |
| `regex /pattern/flags` | Message matches regex |"),

    ("flow", "\
## `flow` — Multi-step dialogue

```gravitix
flow register {
    emit \"What is your name?\"
    let name = wait msg
    emit \"Nice to meet you, {name}!\"
}
```

A `flow` suspends execution at each `wait` and resumes when the user responds.
Start a flow with `run flow name`."),

    ("state", "\
## `state` — Persistent bot state

```gravitix
state {
    count:    int   = 0
    username: str   = \"\"
    active:   bool  = false
}
```

State fields are shared across all handlers. Persist them with `state_save()`."),

    ("every", "\
## `every` — Periodic scheduler

```gravitix
every 30 min {
    emit \"Reminder: check your tasks!\"
}
every 1 hour {
    let data = fetch(\"https://api.example.com/news\")
    emit data.headline
}
```

**Units:** `sec`, `min`, `hour`, `day`"),

    ("at", "\
## `at` — Time-based scheduler

```gravitix
at \"09:00\" {
    emit \"Good morning!\"
}
at \"23:59\" {
    emit \"Day summary: {state.count} events\"
}
```

Time is in `HH:MM` format (24 h)."),

    ("emit", "\
## `emit` — Send a message

```gravitix
emit \"Hello!\"
emit \"Score: {state.score}\"
emit [\"Line 1\", \"Line 2\"]
```

Sends a text message to the current chat.
String interpolation: `\"{variable}\"`."),

    ("wait", "\
## `wait msg` — Await user reply (inside a `flow`)

```gravitix
flow ask_age {
    emit \"How old are you?\"
    let age = wait msg
    emit \"You are {age} years old.\"
}
```

Suspends the current flow until the user sends a message.
Returns the message text as a string."),

    ("keyboard", "\
## `keyboard` — Send inline keyboard

```gravitix
keyboard \"Choose an option:\", [
    [\"Option A\", \"opt_a\"],
    [\"Option B\", \"opt_b\"]
]
```

Each row is `[\"Button label\", \"callback_data\"]`.
Handle button presses with `on callback \"opt_a\" { … }`."),

    ("callback", "\
## `callback` — Inline button handler

```gravitix
on callback \"confirm\" {
    answer \"Confirmed!\"
    emit \"Your order is placed.\"
}
```

`answer` dismisses the loading indicator on the button."),

    ("try", "\
## `try / catch` — Error handling

```gravitix
try {
    let result = fetch(\"https://api.example.com\")
    emit result.data
} catch e {
    emit \"Error: {e}\"
}
```

`e` contains the error message as a string."),

    ("match", "\
## `match` — Pattern matching

```gravitix
match ctx.text {
    \"yes\"     => emit \"Confirmed\"
    \"no\"      => emit \"Cancelled\"
    /^\\d+$/   => emit \"Number: {ctx.text}\"
    _          => emit \"Unknown\"
}
```

Patterns can be string literals, regex literals, or `_` (wildcard)."),

    ("struct", "\
## `struct` — Struct definition

```gravitix
struct Point {
    x: float
    y: float
}

let p = Point { x: 1.0, y: 2.0 }
emit \"({p.x}, {p.y})\"
```"),

    ("test", "\
## `test` — Test block

```gravitix
test \"arithmetic\" {
    assert_eq(1 + 1, 2)
    assert(2 > 1, \"2 should be greater than 1\")
}
```

Run all tests with: `gravitix test file.grav`"),

    ("use", "\
## `use` — Import another script

```gravitix
use \"helpers.grav\"
use \"math_utils.grav\"
```

Loads the specified `.grav` file and makes its functions available."),

    ("let", "\
## `let` — Variable declaration

```gravitix
let x = 42
let name = \"Alice\"
let items = [1, 2, 3]
let config = { host: \"localhost\", port: 8080 }
```

Variables are block-scoped. Reassign with `x = new_value`."),

    ("return", "\
## `return` — Return from function

```gravitix
fn abs(x: float) -> float {
    if x < 0 { return -x }
    return x
}
```"),

    ("break",    "## `break`\n\nBreaks out of the nearest enclosing `while` or `for` loop."),
    ("continue", "## `continue`\n\nSkips to the next iteration of the nearest enclosing loop."),
    ("true",     "## `true`\n\nBoolean literal `true`."),
    ("false",    "## `false`\n\nBoolean literal `false`."),
    ("null",     "## `null`\n\nThe null / absent value."),

    // ── Constants ─────────────────────────────────────────────────────────────
    ("pi",          "## `pi` — Constant\n\n```\npi ≈ 3.14159265358979323846\n```\nRatio of a circle's circumference to its diameter."),
    ("e",           "## `e` — Constant\n\n```\ne ≈ 2.71828182845904523536\n```\nEuler's number — base of the natural logarithm."),
    ("tau",         "## `tau` — Constant\n\n```\ntau = 2π ≈ 6.28318530717958647692\n```\nFull turn in radians."),
    ("inf",         "## `inf` — Constant\n\nPositive infinity (`f64::INFINITY`)."),
    ("phi",         "## `phi` — Constant\n\n```\nφ ≈ 1.61803398874989484820\n```\nThe golden ratio."),
    ("euler_gamma", "## `euler_gamma` — Constant\n\n```\nγ ≈ 0.57721566490153286060\n```\nThe Euler–Mascheroni constant."),

    // ── Core builtins ─────────────────────────────────────────────────────────
    ("len", "\
## `len(v)` — Length

**Signature:** `len(v: str | list | map) -> int`

Returns the number of characters (string), elements (list), or keys (map).

```gravitix
len(\"hello\")      // 5
len([1, 2, 3])   // 3
```"),

    ("type_of", "\
## `type_of(v)` — Type name

**Signature:** `type_of(v: any) -> str`

Returns the type of `v` as a lowercase string:
`\"int\"`, `\"float\"`, `\"str\"`, `\"bool\"`, `\"list\"`, `\"map\"`, `\"null\"`, `\"fn\"`"),

    ("to_int",   "## `to_int(v)`\n\n**Signature:** `to_int(v: any) -> int`\n\nConverts a value to integer. Strings are parsed; floats are truncated."),
    ("to_float", "## `to_float(v)`\n\n**Signature:** `to_float(v: any) -> float`\n\nConverts a value to float."),
    ("to_str",   "## `to_str(v)`\n\n**Signature:** `to_str(v: any) -> str`\n\nConverts any value to its string representation."),

    ("print", "\
## `print(v…)` — Print to stdout

```gravitix
print(\"debug:\", x, y)
```"),

    ("log", "\
## `log(v…)` — Print to stderr

```gravitix
log(\"warn:\", result)
```"),

    ("random",   "## `random()`\n\n**Signature:** `random() -> float`\n\nReturns a uniformly distributed random float in `[0.0, 1.0)`."),
    ("rand_int", "## `rand_int(min, max)`\n\n**Signature:** `rand_int(min: int, max: int) -> int`\n\nReturns a random integer in `[min, max]` (inclusive)."),
    ("min",      "## `min(a, b)` — Minimum\n\nReturns the smaller of two comparable values."),
    ("max",      "## `max(a, b)` — Maximum\n\nReturns the larger of two comparable values."),

    ("map_list", "\
## `map_list(list, fn)` — Transform

**Signature:** `map_list(list: list, fn: fn) -> list`

Applies `fn` to each element and returns a new list.

```gravitix
let doubled = map_list([1, 2, 3], fn(x) { x * 2 })
// [2, 4, 6]
```"),

    ("filter_list", "\
## `filter_list(list, fn)` — Filter

**Signature:** `filter_list(list: list, fn: fn) -> list`

Keeps elements for which `fn` returns truthy.

```gravitix
let evens = filter_list([1,2,3,4], fn(x) { x % 2 == 0 })
// [2, 4]
```"),

    ("fetch", "\
## `fetch(url, …)` — HTTP request

**Signature:** `fetch(url: str, method?: str, body?: any, headers?: map) -> any`

Performs an HTTP request and returns the parsed JSON or raw text.

```gravitix
let res  = fetch(\"https://api.example.com/data\")
let post = fetch(\"https://api.example.com/item\", \"POST\",
                 { title: \"New\" }, { Authorization: \"Bearer token\" })
```"),

    ("json_parse",  "## `json_parse(str)`\n\n**Signature:** `json_parse(s: str) -> any`\n\nParses a JSON string into a Gravitix value (map / list / str / int / float / bool / null)."),
    ("json_encode", "## `json_encode(val)`\n\n**Signature:** `json_encode(v: any) -> str`\n\nSerialises a Gravitix value to a JSON string."),
    ("state_save",  "## `state_save()`\n\nPersists the current `state` block to disk so it survives restarts."),
    ("state_load",  "## `state_load()`\n\nLoads the previously saved `state` from disk."),

    ("assert", "\
## `assert(cond, msg?)` — Assertion

**Signature:** `assert(cond: bool, msg?: str)`

Fails the current test if `cond` is falsy.

```gravitix
assert(x > 0, \"x must be positive\")
```"),

    ("assert_eq", "\
## `assert_eq(a, b, msg?)` — Equality assertion

Fails if `a != b`. Prints both values on failure."),

    ("assert_ne", "\
## `assert_ne(a, b, msg?)` — Inequality assertion

Fails if `a == b`."),

    ("now_unix",    "## `now_unix()`\n\n**Signature:** `now_unix() -> int`\n\nReturns the current UTC time as a Unix timestamp (seconds since epoch)."),
    ("format_date", "## `format_date(ts, fmt?)`\n\n**Signature:** `format_date(ts: int, fmt?: str) -> str`\n\nFormats a Unix timestamp. Default format: `\"%Y-%m-%d %H:%M:%S\"`."),
    ("sleep_ms",    "## `sleep_ms(ms)`\n\n**Signature:** `sleep_ms(ms: int)`\n\nSleeps for `ms` milliseconds. Avoid in handlers; prefer `every`."),

    // ── Math ──────────────────────────────────────────────────────────────────
    ("sin",  "## `sin(x)` — Sine\n\n**Signature:** `sin(x: float) -> float`\n\nReturns the sine of `x` (radians).\n\n```gravitix\nsin(pi / 2)  // 1.0\n```"),
    ("cos",  "## `cos(x)` — Cosine\n\n**Signature:** `cos(x: float) -> float`\n\nReturns the cosine of `x` (radians)."),
    ("tan",  "## `tan(x)` — Tangent\n\n**Signature:** `tan(x: float) -> float`\n\nReturns the tangent of `x` (radians). Undefined at `π/2 + nπ`."),
    ("asin", "## `asin(x)` — Arc sine\n\n**Signature:** `asin(x: float) -> float`\n\nInverse sine, returns value in `[-π/2, π/2]`. Domain: `[-1, 1]`."),
    ("acos", "## `acos(x)` — Arc cosine\n\n**Signature:** `acos(x: float) -> float`\n\nInverse cosine, returns value in `[0, π]`. Domain: `[-1, 1]`."),
    ("atan", "## `atan(x)` — Arc tangent\n\n**Signature:** `atan(x: float) -> float`\n\nInverse tangent, returns value in `(-π/2, π/2)`."),
    ("atan2","## `atan2(y, x)` — 2-argument arc tangent\n\n**Signature:** `atan2(y: float, x: float) -> float`\n\nAngle of the vector `(x, y)` in `(-π, π]`."),
    ("sinh", "## `sinh(x)` — Hyperbolic sine\n\n**Signature:** `sinh(x: float) -> float`"),
    ("cosh", "## `cosh(x)` — Hyperbolic cosine\n\n**Signature:** `cosh(x: float) -> float`"),
    ("tanh", "## `tanh(x)` — Hyperbolic tangent\n\n**Signature:** `tanh(x: float) -> float`\n\nReturns value in `(-1, 1)`."),
    ("sqrt", "## `sqrt(x)` — Square root\n\n**Signature:** `sqrt(x: float) -> float`\n\n```gravitix\nsqrt(9.0)  // 3.0\n```"),
    ("cbrt", "## `cbrt(x)` — Cube root\n\n**Signature:** `cbrt(x: float) -> float`\n\n```gravitix\ncbrt(27.0)  // 3.0\n```"),
    ("pow",  "## `pow(base, exp)` — Power\n\n**Signature:** `pow(base: float, exp: float) -> float`\n\n```gravitix\npow(2.0, 10.0)  // 1024.0\n```"),
    ("ln",   "## `ln(x)` — Natural logarithm\n\n**Signature:** `ln(x: float) -> float`\n\n`x` must be positive."),
    ("log2", "## `log2(x)` — Base-2 logarithm\n\n**Signature:** `log2(x: float) -> float`"),
    ("log10","## `log10(x)` — Base-10 logarithm\n\n**Signature:** `log10(x: float) -> float`"),
    ("exp",  "## `exp(x)` — Exponential\n\n**Signature:** `exp(x: float) -> float`\n\nReturns `e^x`."),
    ("abs",  "## `abs(x)` — Absolute value\n\n**Signature:** `abs(x: number) -> number`\n\nWorks on both `int` and `float`."),
    ("floor","## `floor(x)` — Floor\n\n**Signature:** `floor(x: float) -> float`\n\nLargest integer ≤ `x`."),
    ("ceil", "## `ceil(x)` — Ceiling\n\n**Signature:** `ceil(x: float) -> float`\n\nSmallest integer ≥ `x`."),
    ("round","## `round(x)` — Round\n\n**Signature:** `round(x: float) -> float`\n\nRounds to nearest integer (halfway cases away from zero)."),
    ("sign", "## `sign(x)` — Sign\n\n**Signature:** `sign(x: float) -> float`\n\nReturns `-1.0`, `0.0`, or `1.0`."),
    ("trunc","## `trunc(x)` — Truncate\n\n**Signature:** `trunc(x: float) -> float`\n\nDrops the fractional part (rounds towards zero)."),
    ("fract","## `fract(x)` — Fractional part\n\n**Signature:** `fract(x: float) -> float`\n\nReturns `x - trunc(x)`."),
    ("hypot","## `hypot(x, y)` — Hypotenuse\n\n**Signature:** `hypot(x: float, y: float) -> float`\n\nReturns `sqrt(x² + y²)` without intermediate overflow."),
    ("clamp","## `clamp(x, min, max)` — Clamp\n\n**Signature:** `clamp(x: float, min: float, max: float) -> float`\n\nConstrains `x` to `[min, max]`."),
    ("lerp", "## `lerp(a, b, t)` — Linear interpolation\n\n**Signature:** `lerp(a: float, b: float, t: float) -> float`\n\nReturns `a + t * (b - a)`. `t=0` → `a`, `t=1` → `b`."),
    ("degrees","## `degrees(x)` — Radians → degrees\n\n**Signature:** `degrees(x: float) -> float`"),
    ("radians","## `radians(x)` — Degrees → radians\n\n**Signature:** `radians(x: float) -> float`"),

    // ── Complex ───────────────────────────────────────────────────────────────
    ("complex","## `complex(re, im)` — Create complex number\n\n**Signature:** `complex(re: float, im: float) -> complex`\n\n```gravitix\nlet z = complex(1.0, 2.0)  // 1 + 2i\n```"),
    ("re",     "## `re(z)` — Real part\n\n**Signature:** `re(z: complex) -> float`"),
    ("im",     "## `im(z)` — Imaginary part\n\n**Signature:** `im(z: complex) -> float`"),
    ("conj",   "## `conj(z)` — Complex conjugate\n\n**Signature:** `conj(z: complex) -> complex`\n\nFlips the sign of the imaginary part."),
    ("cabs",   "## `cabs(z)` — Complex magnitude\n\n**Signature:** `cabs(z: complex) -> float`\n\nReturns `sqrt(re² + im²)`."),
    ("arg",    "## `arg(z)` — Complex argument\n\n**Signature:** `arg(z: complex) -> float`\n\nReturns the phase angle in `(-π, π]`."),
    ("polar",  "## `polar(r, theta)` — Polar form → complex\n\n**Signature:** `polar(r: float, theta: float) -> complex`"),
    ("cexp",   "## `cexp(z)` — Complex exponential\n\n**Signature:** `cexp(z: complex) -> complex`\n\nReturns `e^z`."),
    ("clog",   "## `clog(z)` — Complex logarithm\n\n**Signature:** `clog(z: complex) -> complex`\n\nPrincipal branch: imaginary part in `(-π, π]`."),
    ("csin",   "## `csin(z)` — Complex sine\n\n**Signature:** `csin(z: complex) -> complex`"),
    ("ccos",   "## `ccos(z)` — Complex cosine\n\n**Signature:** `ccos(z: complex) -> complex`"),
    ("ctan",   "## `ctan(z)` — Complex tangent\n\n**Signature:** `ctan(z: complex) -> complex`"),
    ("cpow",   "## `cpow(z, w)` — Complex power\n\n**Signature:** `cpow(z: complex, w: complex) -> complex`"),
    ("csqrt",  "## `csqrt(z)` — Complex square root\n\n**Signature:** `csqrt(z: complex) -> complex`\n\nPrincipal branch: non-negative real part."),

    // ── Special functions ─────────────────────────────────────────────────────
    ("gamma",     "## `gamma(x)` — Gamma function\n\n**Signature:** `gamma(x: float) -> float`\n\nGeneralises factorial: `gamma(n+1) = n!` for positive integers."),
    ("lgamma",    "## `lgamma(x)` — Log-gamma\n\n**Signature:** `lgamma(x: float) -> float`\n\nNatural log of `|gamma(x)|`. More numerically stable for large `x`."),
    ("beta",      "## `beta(a, b)` — Beta function\n\n**Signature:** `beta(a: float, b: float) -> float`\n\n`beta(a,b) = gamma(a)*gamma(b) / gamma(a+b)`"),
    ("erf",       "## `erf(x)` — Error function\n\n**Signature:** `erf(x: float) -> float`\n\nUsed in statistics and diffusion problems. Range: `(-1, 1)`."),
    ("erfc",      "## `erfc(x)` — Complementary error function\n\n**Signature:** `erfc(x: float) -> float`\n\n`erfc(x) = 1 - erf(x)`."),
    ("zeta",      "## `zeta(s)` — Riemann zeta function\n\n**Signature:** `zeta(s: float) -> float`\n\nDefined for `s > 1`; analytically continued elsewhere."),
    ("bessel_j",  "## `bessel_j(n, x)` — Bessel function of the first kind\n\n**Signature:** `bessel_j(n: int, x: float) -> float`"),
    ("bessel_y",  "## `bessel_y(n, x)` — Bessel function of the second kind\n\n**Signature:** `bessel_y(n: int, x: float) -> float`"),
    ("legendre",  "## `legendre(n, x)` — Legendre polynomial\n\n**Signature:** `legendre(n: int, x: float) -> float`\n\n`P_n(x)`, defined on `[-1, 1]`."),
    ("hermite",   "## `hermite(n, x)` — Hermite polynomial\n\n**Signature:** `hermite(n: int, x: float) -> float`\n\nPhysicists' `H_n(x)`."),
    ("chebyshev", "## `chebyshev(n, x)` — Chebyshev polynomial\n\n**Signature:** `chebyshev(n: int, x: float) -> float`\n\n`T_n(x)` (first kind), optimal for polynomial approximation."),

    // ── Linear algebra ────────────────────────────────────────────────────────
    ("dot",         "## `dot(a, b)` — Dot product\n\n**Signature:** `dot(a: list, b: list) -> float`\n\nReturns the scalar dot product of two equal-length vectors."),
    ("cross",       "## `cross(a, b)` — Cross product\n\n**Signature:** `cross(a: list, b: list) -> list`\n\n3-D cross product. Both inputs must have length 3."),
    ("norm",        "## `norm(v)` — Euclidean norm\n\n**Signature:** `norm(v: list) -> float`\n\nReturns `sqrt(sum of squares)`."),
    ("normalize",   "## `normalize(v)` — Unit vector\n\n**Signature:** `normalize(v: list) -> list`\n\nDivides each element by `norm(v)`."),
    ("det",         "## `det(m)` — Determinant\n\n**Signature:** `det(m: list) -> float`\n\n`m` is a square matrix represented as a list of rows."),
    ("inv",         "## `inv(m)` — Matrix inverse\n\n**Signature:** `inv(m: list) -> list`\n\nReturns the inverse of square matrix `m`."),
    ("transpose",   "## `transpose(m)` — Transpose\n\n**Signature:** `transpose(m: list) -> list`\n\nFlips rows and columns."),
    ("trace",       "## `trace(m)` — Trace\n\n**Signature:** `trace(m: list) -> float`\n\nSum of diagonal elements."),
    ("mat_mul",     "## `mat_mul(a, b)` — Matrix multiplication\n\n**Signature:** `mat_mul(a: list, b: list) -> list`"),
    ("solve",       "## `solve(A, b)` — Linear system\n\n**Signature:** `solve(A: list, b: list) -> list`\n\nSolves `A·x = b`. Returns `x`."),
    ("eigenvalues", "## `eigenvalues(m)` — Eigenvalues\n\n**Signature:** `eigenvalues(m: list) -> list`\n\nReturns a list of eigenvalues of square matrix `m`."),

    // ── Number theory ─────────────────────────────────────────────────────────
    ("gcd",       "## `gcd(a, b)` — Greatest common divisor\n\n**Signature:** `gcd(a: int, b: int) -> int`"),
    ("lcm",       "## `lcm(a, b)` — Least common multiple\n\n**Signature:** `lcm(a: int, b: int) -> int`"),
    ("factorial", "## `factorial(n)` — Factorial\n\n**Signature:** `factorial(n: int) -> int`\n\n`n!` for `n ≥ 0`."),
    ("is_prime",  "## `is_prime(n)` — Primality test\n\n**Signature:** `is_prime(n: int) -> bool`"),
    ("primes",    "## `primes(n)` — Primes up to n\n\n**Signature:** `primes(n: int) -> list`\n\nReturns all primes ≤ `n` (Sieve of Eratosthenes)."),
    ("fib",       "## `fib(n)` — Fibonacci number\n\n**Signature:** `fib(n: int) -> int`\n\n`fib(0)=0`, `fib(1)=1`."),
    ("binomial",  "## `binomial(n, k)` — Binomial coefficient\n\n**Signature:** `binomial(n: int, k: int) -> int`\n\nReturns `C(n,k) = n! / (k! * (n-k)!)`."),
    ("euler_phi", "## `euler_phi(n)` — Euler's totient\n\n**Signature:** `euler_phi(n: int) -> int`\n\nCounts integers in `[1, n]` coprime to `n`."),

    // ── Statistics ────────────────────────────────────────────────────────────
    ("sum",        "## `sum(list)` — Sum\n\n**Signature:** `sum(list: list) -> float`\n\nReturns the sum of all elements."),
    ("avg",        "## `avg(list)` — Arithmetic mean\n\n**Signature:** `avg(list: list) -> float`"),
    ("median",     "## `median(list)` — Median\n\n**Signature:** `median(list: list) -> float`\n\nMiddle value after sorting."),
    ("variance",   "## `variance(list)` — Variance\n\n**Signature:** `variance(list: list) -> float`\n\nPopulation variance."),
    ("stddev",     "## `stddev(list)` — Standard deviation\n\n**Signature:** `stddev(list: list) -> float`"),
    ("cov",        "## `cov(a, b)` — Covariance\n\n**Signature:** `cov(a: list, b: list) -> float`"),
    ("corr",       "## `corr(a, b)` — Pearson correlation\n\n**Signature:** `corr(a: list, b: list) -> float`\n\nReturns value in `[-1, 1]`."),
    ("normal_pdf", "## `normal_pdf(x, mu, sigma)` — Normal PDF\n\n**Signature:** `normal_pdf(x: float, mu: float, sigma: float) -> float`"),
    ("normal_cdf", "## `normal_cdf(x, mu, sigma)` — Normal CDF\n\n**Signature:** `normal_cdf(x: float, mu: float, sigma: float) -> float`\n\nProbability P(X ≤ x) for a normal distribution."),
    ("linreg",     "## `linreg(xs, ys)` — Linear regression\n\n**Signature:** `linreg(xs: list, ys: list) -> [slope, intercept]`\n\nOrdinary least squares fit of `y = slope*x + intercept`."),

    // ── Transforms ────────────────────────────────────────────────────────────
    ("fft",      "## `fft(list)` — Fast Fourier Transform\n\n**Signature:** `fft(list: list) -> list`\n\nReturns a list of complex numbers (DFT output)."),
    ("ifft",     "## `ifft(list)` — Inverse FFT\n\n**Signature:** `ifft(list: list) -> list`\n\nReturns a list of complex numbers (IDFT output)."),
    ("convolve", "## `convolve(a, b)` — Discrete convolution\n\n**Signature:** `convolve(a: list, b: list) -> list`\n\nLinear convolution of two sequences."),

    // ── Calculus ──────────────────────────────────────────────────────────────
    ("deriv",            "## `deriv(f, x)` — Numerical derivative\n\n**Signature:** `deriv(f: fn, x: float) -> float`\n\nCentral-difference approximation of `f'(x)`."),
    ("integral_trapz",   "## `integral_trapz(f, a, b, n?)` — Trapezoidal integration\n\n**Signature:** `integral_trapz(f: fn, a: float, b: float, n?: int) -> float`\n\nApproximates `∫_a^b f(x) dx` using the trapezoidal rule."),
    ("integral_simpson", "## `integral_simpson(f, a, b, n?)` — Simpson integration\n\n**Signature:** `integral_simpson(f: fn, a: float, b: float, n?: int) -> float`\n\nApproximates `∫_a^b f(x) dx` using Simpson's 1/3 rule."),
    ("diff",             "## `diff(list)` — Finite differences\n\n**Signature:** `diff(list: list) -> list`\n\nReturns consecutive differences: `[a[1]-a[0], a[2]-a[1], …]`."),
    ("cumsum",           "## `cumsum(list)` — Cumulative sum\n\n**Signature:** `cumsum(list: list) -> list`\n\nRunning total of elements."),
    ("taylor_eval",      "## `taylor_eval(coeffs, x)` — Taylor polynomial evaluation\n\n**Signature:** `taylor_eval(coeffs: list, x: float) -> float`\n\n`coeffs[i]` is the coefficient for `x^i`. Uses Horner's method."),
];

// ─────────────────────────────────────────────────────────────────────────────
// Signature help
// ─────────────────────────────────────────────────────────────────────────────

fn signature_help(src: &str, line: usize, col: usize) -> Option<JVal> {
    let lines: Vec<&str> = src.lines().collect();
    let line_str = *lines.get(line)?;

    // Find the active parameter count (how many commas before cursor at this call depth)
    let prefix = &line_str[..col.min(line_str.len())];
    // Walk backwards to find the opening '('
    let mut depth = 0i32;
    let mut comma_count = 0usize;
    for ch in prefix.chars().rev() {
        match ch {
            ')' | ']' => depth += 1,
            '(' | '[' => {
                if depth == 0 { break; }
                depth -= 1;
            }
            ',' if depth == 0 => comma_count += 1,
            _ => {}
        }
    }

    // Find the function name before the '('
    let before_paren = prefix.trim_end_matches(|c: char| c != '(');
    let func_name = before_paren
        .trim_end_matches('(')
        .trim()
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or("")
        .to_string();

    if func_name.is_empty() { return None; }

    let sig = SIGNATURE_DB.iter().find(|(name, _)| *name == func_name)?;
    let (_, entries) = sig;

    let sigs: Vec<JVal> = entries.iter().map(|(label, doc, params)| {
        let params_json: Vec<JVal> = params.iter().map(|(pl, pd)| json!({
            "label": pl,
            "documentation": { "kind": "markdown", "value": pd }
        })).collect();
        json!({
            "label": label,
            "documentation": { "kind": "markdown", "value": doc },
            "parameters": params_json
        })
    }).collect();

    Some(json!({
        "signatures": sigs,
        "activeSignature": 0,
        "activeParameter": comma_count
    }))
}

// signature DB: (fn_name, [(label, doc, [(param_label, param_doc)])])
static SIGNATURE_DB: &[(&str, &[(&str, &str, &[(&str, &str)])])] = &[
    ("sin",  &[("sin(x: float) -> float",  "Returns the **sine** of `x` (in radians).",   &[("x", "Angle in radians")])]),
    ("cos",  &[("cos(x: float) -> float",  "Returns the **cosine** of `x` (in radians).", &[("x", "Angle in radians")])]),
    ("tan",  &[("tan(x: float) -> float",  "Returns the **tangent** of `x` (in radians).", &[("x", "Angle in radians")])]),
    ("asin", &[("asin(x: float) -> float", "Arc sine, result in `[-π/2, π/2]`.",           &[("x", "Value in [-1, 1]")])]),
    ("acos", &[("acos(x: float) -> float", "Arc cosine, result in `[0, π]`.",              &[("x", "Value in [-1, 1]")])]),
    ("atan", &[("atan(x: float) -> float", "Arc tangent, result in `(-π/2, π/2)`.",        &[("x", "Float value")])]),
    ("atan2",&[("atan2(y: float, x: float) -> float", "2-argument arc tangent; angle of vector `(x,y)`.", &[("y", "y component"), ("x", "x component")])]),
    ("sinh", &[("sinh(x: float) -> float", "Hyperbolic sine.",  &[("x", "Float value")])]),
    ("cosh", &[("cosh(x: float) -> float", "Hyperbolic cosine.", &[("x", "Float value")])]),
    ("tanh", &[("tanh(x: float) -> float", "Hyperbolic tangent, result in `(-1, 1)`.", &[("x", "Float value")])]),
    ("sqrt", &[("sqrt(x: float) -> float", "Square root of `x`. `x` must be ≥ 0.", &[("x", "Non-negative float")])]),
    ("cbrt", &[("cbrt(x: float) -> float", "Cube root of `x`.", &[("x", "Float value")])]),
    ("pow",  &[("pow(base: float, exp: float) -> float", "Returns `base ^ exp`.", &[("base", "Base"), ("exp", "Exponent")])]),
    ("ln",   &[("ln(x: float) -> float",   "Natural logarithm.", &[("x", "Positive float")])]),
    ("log2", &[("log2(x: float) -> float", "Base-2 logarithm.",  &[("x", "Positive float")])]),
    ("log10",&[("log10(x: float) -> float","Base-10 logarithm.", &[("x", "Positive float")])]),
    ("exp",  &[("exp(x: float) -> float",  "e raised to the power `x`.", &[("x", "Float value")])]),
    ("abs",  &[("abs(x: number) -> number","Absolute value.",    &[("x", "Int or float")])]),
    ("floor",&[("floor(x: float) -> float","Floor (round down).", &[("x", "Float value")])]),
    ("ceil", &[("ceil(x: float) -> float", "Ceiling (round up).", &[("x", "Float value")])]),
    ("round",&[("round(x: float) -> float","Round to nearest integer.", &[("x", "Float value")])]),
    ("clamp",&[("clamp(x: float, min: float, max: float) -> float", "Clamps `x` to `[min, max]`.", &[("x", "Value to clamp"), ("min", "Lower bound"), ("max", "Upper bound")])]),
    ("lerp", &[("lerp(a: float, b: float, t: float) -> float", "Linear interpolation between `a` and `b` at `t`.", &[("a", "Start value"), ("b", "End value"), ("t", "Blend factor [0, 1]")])]),
    ("hypot",&[("hypot(x: float, y: float) -> float", "Euclidean distance from origin.", &[("x", "x component"), ("y", "y component")])]),
    ("complex", &[("complex(re: float, im: float) -> complex", "Creates a complex number `re + im·i`.", &[("re", "Real part"), ("im", "Imaginary part")])]),
    ("polar",   &[("polar(r: float, theta: float) -> complex", "Creates a complex number from polar coordinates.", &[("r", "Magnitude"), ("theta", "Angle in radians")])]),
    ("cpow",    &[("cpow(z: complex, w: complex) -> complex", "Complex power `z^w`.", &[("z", "Base (complex)"), ("w", "Exponent (complex)")])]),
    ("gamma",   &[("gamma(x: float) -> float", "Gamma function — generalises factorial.", &[("x", "Float value (not 0 or negative integer)")])]),
    ("beta",    &[("beta(a: float, b: float) -> float", "Beta function.", &[("a", "First parameter"), ("b", "Second parameter")])]),
    ("bessel_j",&[("bessel_j(n: int, x: float) -> float", "Bessel function J_n(x) of the first kind.", &[("n", "Order (integer)"), ("x", "Argument")])]),
    ("bessel_y",&[("bessel_y(n: int, x: float) -> float", "Bessel function Y_n(x) of the second kind.", &[("n", "Order (integer)"), ("x", "Argument > 0")])]),
    ("legendre",&[("legendre(n: int, x: float) -> float", "Legendre polynomial P_n(x).", &[("n", "Degree (non-negative integer)"), ("x", "Value in [-1, 1]")])]),
    ("dot",     &[("dot(a: list, b: list) -> float", "Dot product of two vectors.", &[("a", "First vector"), ("b", "Second vector (same length)")])]),
    ("cross",   &[("cross(a: list, b: list) -> list", "3-D cross product.", &[("a", "3-element vector"), ("b", "3-element vector")])]),
    ("mat_mul", &[("mat_mul(a: list, b: list) -> list", "Matrix multiplication.", &[("a", "Left matrix (list of rows)"), ("b", "Right matrix (list of rows)")])]),
    ("solve",   &[("solve(A: list, b: list) -> list", "Solves A·x = b.", &[("A", "Coefficient matrix"), ("b", "RHS vector")])]),
    ("gcd",     &[("gcd(a: int, b: int) -> int", "Greatest common divisor.", &[("a", "First integer"), ("b", "Second integer")])]),
    ("lcm",     &[("lcm(a: int, b: int) -> int", "Least common multiple.", &[("a", "First integer"), ("b", "Second integer")])]),
    ("binomial",&[("binomial(n: int, k: int) -> int", "Binomial coefficient C(n, k).", &[("n", "Total elements"), ("k", "Chosen elements")])]),
    ("cov",     &[("cov(a: list, b: list) -> float", "Covariance of two datasets.", &[("a", "First sample"), ("b", "Second sample (same length)")])]),
    ("corr",    &[("corr(a: list, b: list) -> float", "Pearson correlation in [-1, 1].", &[("a", "First sample"), ("b", "Second sample (same length)")])]),
    ("normal_pdf",&[("normal_pdf(x: float, mu: float, sigma: float) -> float", "Normal distribution PDF.", &[("x", "Evaluation point"), ("mu", "Mean"), ("sigma", "Standard deviation")])]),
    ("normal_cdf",&[("normal_cdf(x: float, mu: float, sigma: float) -> float", "Normal distribution CDF.", &[("x", "Evaluation point"), ("mu", "Mean"), ("sigma", "Standard deviation")])]),
    ("linreg",  &[("linreg(xs: list, ys: list) -> list", "Ordinary least squares. Returns `[slope, intercept]`.", &[("xs", "x data points"), ("ys", "y data points (same length)")])]),
    ("deriv",   &[("deriv(f: fn, x: float) -> float", "Numerical derivative f'(x) by central differences.", &[("f", "Function to differentiate"), ("x", "Point of evaluation")])]),
    ("integral_trapz",  &[("integral_trapz(f: fn, a: float, b: float, n?: int) -> float", "Trapezoidal quadrature of f on [a, b].", &[("f", "Integrand"), ("a", "Left endpoint"), ("b", "Right endpoint"), ("n", "Number of subintervals (default 1000)")])]),
    ("integral_simpson",&[("integral_simpson(f: fn, a: float, b: float, n?: int) -> float", "Simpson quadrature of f on [a, b].", &[("f", "Integrand"), ("a", "Left endpoint"), ("b", "Right endpoint"), ("n", "Number of subintervals (default 1000, must be even)")])]),
    ("taylor_eval",&[("taylor_eval(coeffs: list, x: float) -> float", "Evaluates a Taylor polynomial at x.", &[("coeffs", "Coefficients [c0, c1, …] for c0 + c1*x + …"), ("x", "Evaluation point")])]),
    ("fetch",   &[("fetch(url: str, method?: str, body?: any, headers?: map) -> any", "HTTP request.", &[("url", "Request URL"), ("method", "HTTP method (default \"GET\")"), ("body", "Request body"), ("headers", "Additional headers map")])]),
    ("rand_int",&[("rand_int(min: int, max: int) -> int", "Random integer in [min, max] inclusive.", &[("min", "Lower bound (inclusive)"), ("max", "Upper bound (inclusive)")])]),
    ("assert",  &[("assert(cond: bool, msg?: str)", "Fails the test if cond is false.", &[("cond", "Condition to test"), ("msg", "Optional failure message")])]),
    ("assert_eq",&[("assert_eq(a: any, b: any, msg?: str)", "Fails if a != b.", &[("a", "Expected value"), ("b", "Actual value"), ("msg", "Optional failure message")])]),
    ("assert_ne",&[("assert_ne(a: any, b: any, msg?: str)", "Fails if a == b.", &[("a", "First value"), ("b", "Second value"), ("msg", "Optional failure message")])]),
];

// ─────────────────────────────────────────────────────────────────────────────
// Go to definition
// ─────────────────────────────────────────────────────────────────────────────

fn find_definition(src: &str, uri: &str, name: &str) -> Option<JVal> {
    if name.is_empty() { return None; }
    let patterns = [
        format!("fn {}(", name),
        format!("flow {} ", name),
        format!("flow {}\t", name),
        format!("flow {}\n", name),
        format!("flow {}{{", name),
        format!("struct {} ", name),
        format!("struct {}\t", name),
        format!("struct {}\n", name),
        format!("struct {}{{", name),
    ];
    for (line_idx, line) in src.lines().enumerate() {
        for pat in &patterns {
            if line.contains(pat.as_str()) {
                let col = line.find(pat.as_str()).unwrap_or(0);
                return Some(json!({
                    "uri": uri,
                    "range": {
                        "start": { "line": line_idx, "character": col },
                        "end":   { "line": line_idx, "character": col + name.len() }
                    }
                }));
            }
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Document symbols
// ─────────────────────────────────────────────────────────────────────────────

fn document_symbols(src: &str) -> Vec<JVal> {
    document_symbols_with_uri(src, "")
        .into_iter()
        .map(|mut s| { s.as_object_mut().map(|o| o.remove("containerName")); s })
        .collect()
}

fn document_symbols_with_uri(src: &str, uri: &str) -> Vec<JVal> {
    let mut symbols: Vec<JVal> = Vec::new();

    for (line_idx, line) in src.lines().enumerate() {
        let trimmed = line.trim();

        // fn name(
        if let Some(rest) = trimmed.strip_prefix("fn ") {
            if let Some(paren) = rest.find('(') {
                let name = rest[..paren].trim().to_string();
                if !name.is_empty() {
                    symbols.push(make_symbol(&name, 12, line_idx, uri));
                }
            }
        }
        // on /cmd  or  on msg  or  on callback
        else if let Some(rest) = trimmed.strip_prefix("on ") {
            let trigger = rest.split_whitespace().next().unwrap_or("").trim_matches('{').to_string();
            if !trigger.is_empty() {
                symbols.push(make_symbol(&format!("on {trigger}"), 24, line_idx, uri));
            }
        }
        // flow name
        else if let Some(rest) = trimmed.strip_prefix("flow ") {
            let name = rest.split_whitespace().next().unwrap_or("").trim_matches('{').to_string();
            if !name.is_empty() {
                symbols.push(make_symbol(&name, 2, line_idx, uri));
            }
        }
        // state {
        else if trimmed.starts_with("state") && (trimmed.contains('{') || trimmed == "state") {
            symbols.push(make_symbol("state", 13, line_idx, uri));
        }
        // struct Name
        else if let Some(rest) = trimmed.strip_prefix("struct ") {
            let name = rest.split_whitespace().next().unwrap_or("").trim_matches('{').to_string();
            if !name.is_empty() {
                symbols.push(make_symbol(&name, 23, line_idx, uri));
            }
        }
        // test "name"
        else if let Some(rest) = trimmed.strip_prefix("test ") {
            if let Some(quoted) = extract_quoted(rest) {
                symbols.push(make_symbol(&format!("test \"{quoted}\""), 7, line_idx, uri));
            }
        }
        // every N unit
        else if let Some(rest) = trimmed.strip_prefix("every ") {
            let label = rest.splitn(3, ' ').take(2).collect::<Vec<_>>().join(" ");
            if !label.is_empty() {
                symbols.push(make_symbol(&format!("every {label}"), 24, line_idx, uri));
            }
        }
        // at "time"
        else if let Some(rest) = trimmed.strip_prefix("at ") {
            if let Some(t) = extract_quoted(rest) {
                symbols.push(make_symbol(&format!("at \"{t}\""), 24, line_idx, uri));
            }
        }
    }

    symbols
}

fn make_symbol(name: &str, kind: u32, line: usize, uri: &str) -> JVal {
    json!({
        "name": name,
        "kind": kind,
        "containerName": uri,
        "location": {
            "uri": uri,
            "range": {
                "start": { "line": line, "character": 0 },
                "end":   { "line": line, "character": name.len() }
            }
        }
    })
}

fn extract_quoted(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let end = s[start..].find('"')? + start;
    Some(s[start..end].to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Document formatting
// ─────────────────────────────────────────────────────────────────────────────

fn format_document(src: &str) -> Option<JVal> {
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::fmt::format_program;

    let tokens = Lexer::new(src).tokenize().ok()?;
    let prog   = Parser::new(tokens).parse().ok()?;
    let formatted = format_program(&prog);

    let line_count = src.lines().count().max(1);
    let last_line  = src.lines().last().unwrap_or("");

    Some(json!({
        "range": {
            "start": { "line": 0, "character": 0 },
            "end":   { "line": line_count - 1, "character": last_line.len() }
        },
        "newText": formatted
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Code lens
// ─────────────────────────────────────────────────────────────────────────────

fn code_lenses(src: &str) -> Vec<JVal> {
    let mut lenses: Vec<JVal> = Vec::new();

    for (line_idx, line) in src.lines().enumerate() {
        let trimmed = line.trim();

        // test "name" → ▶ Run test
        if let Some(rest) = trimmed.strip_prefix("test ") {
            let test_name = extract_quoted(rest).unwrap_or_else(|| "test".to_string());
            lenses.push(json!({
                "range": {
                    "start": { "line": line_idx, "character": 0 },
                    "end":   { "line": line_idx, "character": line.len() }
                },
                "command": {
                    "title": "▶ Run test",
                    "command": "gravitix.runTest",
                    "arguments": [test_name]
                }
            }));
        }

        // fn name( → ⟨fn⟩ name
        if let Some(rest) = trimmed.strip_prefix("fn ") {
            if let Some(paren) = rest.find('(') {
                let name = rest[..paren].trim().to_string();
                if !name.is_empty() {
                    lenses.push(json!({
                        "range": {
                            "start": { "line": line_idx, "character": 0 },
                            "end":   { "line": line_idx, "character": line.len() }
                        },
                        "command": {
                            "title": format!("⟨fn⟩ {name}"),
                            "command": "gravitix.showFn",
                            "arguments": [name]
                        }
                    }));
                }
            }
        }
    }

    lenses
}

// ─────────────────────────────────────────────────────────────────────────────
// Inlay hints
// ─────────────────────────────────────────────────────────────────────────────

fn inlay_hints(src: &str) -> Vec<JVal> {
    let mut hints: Vec<JVal> = Vec::new();

    // Simple scan: let name = <number_literal>
    // Returns Some(true) = float, Some(false) = int, None = not a simple numeric let.
    let detect_let_type = |line: &str| -> Option<bool> {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix("let ")?;
        // Only emit hint when there is no explicit type annotation
        if rest.contains(':') { return None; }
        let eq_pos = rest.find('=')?;
        let value_part = rest[eq_pos + 1..].trim();
        if value_part.is_empty() { return None; }
        // Integer literal (digits only, optional leading minus)
        if value_part.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return Some(false);
        }
        // Float literal (has dot or 'e')
        let looks_float = value_part.contains('.')
            || (value_part.contains('e')
                && value_part.chars().next().map(|c| c.is_ascii_digit() || c == '-').unwrap_or(false));
        if looks_float && value_part.chars().all(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == '-' || c == '+') {
            return Some(true);
        }
        None
    };

    for (line_idx, line) in src.lines().enumerate() {
        if let Some(is_float) = detect_let_type(line) {
            let type_str = if is_float { ": float" } else { ": int" };
            // Position hint at end of the assignment value
            let char_pos = line.len();
            hints.push(json!({
                "position": { "line": line_idx, "character": char_pos },
                "label": type_str,
                "kind": 1,
                "paddingLeft": true
            }));
        }
    }

    hints
}

// ─────────────────────────────────────────────────────────────────────────────
// Semantic tokens
// ─────────────────────────────────────────────────────────────────────────────
//
// Token type indices (must match legend in declare_capabilities):
//   0 = keyword, 1 = number, 2 = string, 3 = function, 4 = variable, 5 = operator, 6 = comment

const KEYWORDS_SET: &[&str] = &[
    "fn", "on", "flow", "state", "every", "at", "let", "emit", "return",
    "if", "elif", "else", "while", "for", "in", "match", "run", "try", "catch",
    "keyboard", "edit", "answer", "struct", "use", "test", "wait", "guard",
    "callback", "break", "continue", "null", "true", "false",
];

fn semantic_tokens(src: &str) -> Vec<u32> {
    let mut result: Vec<u32> = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for (line_idx, line) in src.lines().enumerate() {
        let line_u32 = line_idx as u32;

        // Skip blank / comment-only lines quickly
        let trimmed = line.trim_start();

        // Line comment  //…
        if trimmed.starts_with("//") {
            let col = (line.len() - trimmed.len()) as u32;
            let delta_line  = line_u32 - prev_line;
            let delta_start = if delta_line == 0 { col - prev_start } else { col };
            result.extend_from_slice(&[delta_line, delta_start, line.len() as u32, 6, 0]);
            prev_line  = line_u32;
            prev_start = col;
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        let n = chars.len();
        let mut i = 0usize;

        while i < n {
            let ch = chars[i];

            // Skip whitespace
            if ch.is_whitespace() { i += 1; continue; }

            // String literal  "…"  (single-line only)
            if ch == '"' {
                let start = i as u32;
                i += 1;
                while i < n && chars[i] != '"' {
                    if chars[i] == '\\' { i += 1; }
                    i += 1;
                }
                if i < n { i += 1; } // closing quote
                let length = i as u32 - start;
                let delta_line  = line_u32 - prev_line;
                let delta_start = if delta_line == 0 { start - prev_start } else { start };
                result.extend_from_slice(&[delta_line, delta_start, length, 2, 0]);
                prev_line  = line_u32;
                prev_start = start;
                continue;
            }

            // Number literal
            if ch.is_ascii_digit() || (ch == '-' && i + 1 < n && chars[i+1].is_ascii_digit()) {
                let start = i as u32;
                if ch == '-' { i += 1; }
                while i < n && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == '_') {
                    i += 1;
                }
                let length = i as u32 - start;
                let delta_line  = line_u32 - prev_line;
                let delta_start = if delta_line == 0 { start - prev_start } else { start };
                result.extend_from_slice(&[delta_line, delta_start, length, 1, 0]);
                prev_line  = line_u32;
                prev_start = start;
                continue;
            }

            // Identifier: keyword, function call, or variable
            if ch.is_alphabetic() || ch == '_' {
                let start = i as u32;
                while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start as usize..i].iter().collect();
                let length = i as u32 - start;

                // Determine token type
                let token_type: u32 = if KEYWORDS_SET.contains(&word.as_str()) {
                    0 // keyword
                } else if i < n && chars[i] == '(' {
                    3 // function call
                } else {
                    4 // variable
                };

                let delta_line  = line_u32 - prev_line;
                let delta_start = if delta_line == 0 { start - prev_start } else { start };
                result.extend_from_slice(&[delta_line, delta_start, length, token_type, 0]);
                prev_line  = line_u32;
                prev_start = start;
                continue;
            }

            i += 1;
        }
    }

    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the text of the document from params["textDocument"]["uri"] via the docs map.
fn doc_text(docs: &HashMap<String, String>, params: &JVal) -> String {
    params["textDocument"]["uri"].as_str()
        .and_then(|u| docs.get(u))
        .cloned()
        .unwrap_or_default()
}

/// Return the identifier/keyword word under (line, col) in src.
fn word_at(src: &str, line: usize, col: usize) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let line_str = lines.get(line).copied().unwrap_or("");
    let chars: Vec<char> = line_str.chars().collect();
    let col = col.min(chars.len());
    let start = chars[..col]
        .iter()
        .rposition(|c| !c.is_alphanumeric() && *c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = chars[col..]
        .iter()
        .position(|c| !c.is_alphanumeric() && *c != '_')
        .map(|i| col + i)
        .unwrap_or(chars.len());
    chars[start..end].iter().collect()
}

fn make_response(id: Option<JVal>, result: JVal) -> JVal {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(JVal::Null),
        "result": result
    })
}

fn send(out: &mut impl Write, msg: &JVal) {
    let body = serde_json::to_string(msg).unwrap_or_default();
    write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body).ok();
    out.flush().ok();
}
