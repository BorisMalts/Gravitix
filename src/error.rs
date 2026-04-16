use colored::*;
use thiserror::Error;

// ─────────────────────────────────────────────────────────────────────────────
// Core error type (unchanged variants — all existing code keeps working)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum GravError {
    #[error("Syntax error at {line}:{col}: {msg}")]
    Syntax { line: u32, col: u32, msg: String },

    #[error("Runtime error: {0}")]
    Runtime(String),

    #[error("Type error: expected {expected}, got {got}")]
    Type { expected: String, got: String },

    #[error("Undefined variable: '{0}'")]
    UndefinedVar(String),

    #[error("Undefined function: '{0}'")]
    UndefinedFn(String),

    #[error("Arity error: '{name}' expects {expected} args, got {got}")]
    Arity { name: String, expected: usize, got: usize },

    #[error("Bot error: {0}")]
    Bot(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type GravResult<T> = Result<T, GravError>;

#[macro_export]
macro_rules! runtime_err {
    ($($arg:tt)*) => {
        $crate::error::GravError::Runtime(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! type_err {
    ($expected:expr, $got:expr) => {
        $crate::error::GravError::Type {
            expected: $expected.to_string(),
            got:      $got.to_string(),
        }
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// Rich diagnostic system
// ─────────────────────────────────────────────────────────────────────────────

/// Source location for error reporting
#[derive(Debug, Clone, Default)]
pub struct Span {
    pub line:   u32,
    pub col:    u32,
    pub len:    u32,       // length of the problematic token/span
    pub source: String,    // the source line text
}

/// A rich diagnostic message
#[derive(Debug)]
pub struct Diagnostic {
    pub kind:    DiagKind,
    pub code:    Option<String>,    // e.g. "G001"
    pub message: String,
    pub span:    Option<Span>,
    pub hint:    Option<String>,
    pub note:    Option<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum DiagKind {
    Error,
    Warning,
    Help,
}

// ─────────────────────────────────────────────────────────────────────────────
// GravError -> Diagnostic conversion
// ─────────────────────────────────────────────────────────────────────────────

impl GravError {
    /// Convert to a rich Diagnostic with error code and suggestions
    pub fn to_diagnostic(&self, _filename: &str, source: &str) -> Diagnostic {
        match self {
            GravError::Syntax { line, col, msg } => {
                let source_line = source.lines().nth((*line as usize).saturating_sub(1))
                    .unwrap_or("").to_string();
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some(categorize_syntax_error(msg)),
                    message: msg.clone(),
                    span: Some(Span {
                        line: *line,
                        col: *col,
                        len: estimate_token_len(msg),
                        source: source_line,
                    }),
                    hint: suggest_syntax_fix(msg),
                    note: None,
                }
            }
            GravError::UndefinedVar(name) => {
                // The name may contain " (did you mean `x`?)" appended by the interpreter
                let clean_name = name.split(" (did you mean").next().unwrap_or(name);
                let hint = if name.contains("did you mean") {
                    // Extract the suggestion that was already embedded
                    name.split("(").nth(1).map(|s| s.trim_end_matches(')').to_string())
                } else {
                    None
                };
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some("G010".into()),
                    message: format!("undefined variable `{}`", clean_name),
                    span: None,
                    hint,
                    note: None,
                }
            }
            GravError::UndefinedFn(name) => {
                let clean_name = name.split(" (did you mean").next().unwrap_or(name);
                let hint = if name.contains("did you mean") {
                    name.split("(").nth(1).map(|s| s.trim_end_matches(')').to_string())
                } else {
                    None
                };
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some("G011".into()),
                    message: format!("undefined function `{}`", clean_name),
                    span: None,
                    hint,
                    note: None,
                }
            }
            GravError::Type { expected, got } => {
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some("G020".into()),
                    message: format!("type mismatch: expected `{}`, got `{}`", expected, got),
                    span: None,
                    hint: Some(format!("convert with `{}()` or check your types", expected)),
                    note: None,
                }
            }
            GravError::Arity { name, expected, got } => {
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some("G030".into()),
                    message: format!("function `{}` takes {} argument{}, but {} {} provided",
                        name, expected,
                        if *expected != 1 { "s" } else { "" },
                        got,
                        if *got != 1 { "were" } else { "was" }),
                    span: None,
                    hint: None,
                    note: None,
                }
            }
            GravError::Runtime(msg) => {
                let code = categorize_runtime_error(msg);
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some(code),
                    message: msg.clone(),
                    span: None,
                    hint: suggest_runtime_fix(msg),
                    note: None,
                }
            }
            GravError::Bot(msg) => {
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some("G090".into()),
                    message: format!("bot error: {}", msg),
                    span: None,
                    hint: None,
                    note: None,
                }
            }
            GravError::Io(e) => {
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some("G091".into()),
                    message: format!("I/O error: {}", e),
                    span: None,
                    hint: suggest_io_fix(e),
                    note: None,
                }
            }
            GravError::Http(e) => {
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some("G092".into()),
                    message: format!("HTTP error: {}", e),
                    span: None,
                    hint: Some("check the URL and network connection".into()),
                    note: None,
                }
            }
            GravError::Json(e) => {
                Diagnostic {
                    kind: DiagKind::Error,
                    code: Some("G093".into()),
                    message: format!("JSON error: {}", e),
                    span: None,
                    hint: Some("check that the data is valid JSON".into()),
                    note: None,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Diagnostic display — Rust-style error output
// ─────────────────────────────────────────────────────────────────────────────

impl Diagnostic {
    pub fn display(&self, filename: &str) -> String {
        let mut out = String::new();

        // Line 1: error[G001]: message
        let kind_str = match self.kind {
            DiagKind::Error   => "error".red().bold().to_string(),
            DiagKind::Warning => "warning".yellow().bold().to_string(),
            DiagKind::Help    => "help".cyan().bold().to_string(),
        };
        let code_str = self.code.as_ref()
            .map(|c| format!("[{}]", c))
            .unwrap_or_default();
        out.push_str(&format!("{}{}: {}\n", kind_str, code_str.bold(), self.message.bold()));

        // Line 2+: source location and underline
        if let Some(ref span) = self.span {
            let line_num = format!("{}", span.line);
            let padding = " ".repeat(line_num.len());

            out.push_str(&format!("{}{} {}:{}:{}\n",
                &padding, " -->".blue().bold(), filename, span.line, span.col));
            out.push_str(&format!("{} {}\n", &padding, "|".blue().bold()));
            out.push_str(&format!("{} {} {}\n",
                line_num.blue().bold(), "|".blue().bold(), span.source));

            // Underline
            let col = (span.col as usize).saturating_sub(1);
            let underline_len = (span.len as usize).max(1);
            let carets = match self.kind {
                DiagKind::Error   => "^".repeat(underline_len).red().bold().to_string(),
                DiagKind::Warning => "^".repeat(underline_len).yellow().bold().to_string(),
                DiagKind::Help    => "^".repeat(underline_len).cyan().bold().to_string(),
            };
            let hint_inline = self.hint.as_ref().map(|h| format!(" {}", h)).unwrap_or_default();
            out.push_str(&format!("{} {} {}{}{}\n",
                &padding, "|".blue().bold(), " ".repeat(col), carets, hint_inline));
            out.push_str(&format!("{} {}\n", &padding, "|".blue().bold()));
        }

        // Note
        if let Some(ref note) = self.note {
            let padding = if let Some(ref span) = self.span {
                " ".repeat(format!("{}", span.line).len())
            } else { " ".into() };
            out.push_str(&format!("{} {} {}: {}\n",
                &padding, "=".blue().bold(), "note".bold(), note));
        }

        // Hint (standalone, when no span is present)
        if self.span.is_none() {
            if let Some(ref hint) = self.hint {
                out.push_str(&format!("  {} {}: {}\n", "=".blue().bold(), "help".bold(), hint));
            }
        }

        out
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// "Did you mean?" — Levenshtein distance
// ─────────────────────────────────────────────────────────────────────────────

/// Levenshtein edit distance between two strings
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i-1] == b[j-1] {
                dp[i-1][j-1]
            } else {
                1 + dp[i-1][j-1].min(dp[i-1][j]).min(dp[i][j-1])
            };
        }
    }
    dp[m][n]
}

/// Find the closest match from a list of candidates
pub fn did_you_mean<'a>(input: &str, candidates: &'a [String]) -> Option<&'a str> {
    let max_dist = match input.len() {
        0..=2 => 1,
        3..=5 => 2,
        _ => 3,
    };
    candidates.iter()
        .map(|c| (c, levenshtein(input, c)))
        .filter(|(_, d)| *d <= max_dist && *d > 0)
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c.as_str())
}

// ─────────────────────────────────────────────────────────────────────────────
// Error categorization and suggestions
// ─────────────────────────────────────────────────────────────────────────────

fn categorize_syntax_error(msg: &str) -> String {
    if msg.contains("unexpected") && msg.contains("expected") { return "G001".into(); }
    if msg.contains("unterminated") { return "G002".into(); }
    if msg.contains("expected identifier") { return "G003".into(); }
    if msg.contains("expected LBrace") || msg.contains("expected '{'")
        || msg.contains("expected RBrace") || msg.contains("expected '}'") { return "G004".into(); }
    if msg.contains("unknown trigger") { return "G005".into(); }
    if msg.contains("ratelimit") { return "G006".into(); }
    "G000".into()
}

fn categorize_runtime_error(msg: &str) -> String {
    if msg.contains("division by zero") || msg.contains("modulo by zero") { return "G040".into(); }
    if msg.contains("index out of") { return "G041".into(); }
    if msg.contains("stack overflow") || msg.contains("recursion") { return "G042".into(); }
    if msg.contains("timeout") { return "G043".into(); }
    if msg.contains("cannot add") || msg.contains("cannot multiply")
        || msg.contains("cannot apply") { return "G044".into(); }
    if msg.contains("circuit breaker") { return "G045".into(); }
    if msg.contains("sandbox") { return "G046".into(); }
    "G050".into()
}

fn suggest_syntax_fix(msg: &str) -> Option<String> {
    if msg.contains("expected LBrace") || msg.contains("expected '{'") {
        return Some("you may have forgotten an opening brace `{`".into());
    }
    if msg.contains("expected RBrace") || msg.contains("expected '}'") {
        return Some("you may have forgotten a closing brace `}`".into());
    }
    if msg.contains("expected Semi") || msg.contains("expected ';'") {
        return Some("semicolons are optional in Gravitix -- check for syntax before this point".into());
    }
    if msg.contains("unterminated string") {
        return Some("add a closing `\"` to terminate the string".into());
    }
    if msg.contains("unterminated regex") {
        return Some("add a closing `/` to terminate the regex".into());
    }
    if msg.contains("unterminated interpolation") {
        return Some("add a closing `}` to terminate the `{expr}` interpolation".into());
    }
    if msg.contains("expected identifier") {
        return Some("a name (identifier) is required here".into());
    }
    if msg.contains("unknown trigger") {
        return Some("valid triggers: /command, msg, callback, join, leave, edited, file, image, voice, reaction, dm, mention, idle, webhook, intent, any".into());
    }
    if msg.contains("expected trigger") {
        return Some("use a trigger like: /start, msg, callback, join, leave".into());
    }
    if msg.contains("decorators must precede") {
        return Some("move the @decorator directly before a `fn` definition".into());
    }
    None
}

fn suggest_runtime_fix(msg: &str) -> Option<String> {
    if msg.contains("division by zero") || msg.contains("modulo by zero") {
        return Some("add a check: `if divisor != 0 { ... }`".into());
    }
    if msg.contains("index out of") {
        return Some("check list length with `len()` before indexing".into());
    }
    if msg.contains("null") || msg.contains("Null") {
        return Some("use `?? default_value` to handle null, or check with `is_null()`".into());
    }
    if msg.contains("cannot add") || msg.contains("cannot multiply") {
        return Some("check that both operands have compatible types (int, float, str)".into());
    }
    if msg.contains("circuit breaker") && msg.contains("open") {
        return Some("the circuit breaker tripped due to previous failures -- wait before retrying".into());
    }
    if msg.contains("'ctx' not available") {
        return Some("`ctx` is only available inside `on` handlers, not in top-level code or standalone functions".into());
    }
    if msg.contains("sandbox") && msg.contains("denied") {
        return Some("sandboxed code cannot call I/O or network functions".into());
    }
    None
}

fn suggest_io_fix(e: &std::io::Error) -> Option<String> {
    match e.kind() {
        std::io::ErrorKind::NotFound => Some("check that the file path is correct".into()),
        std::io::ErrorKind::PermissionDenied => Some("check file permissions".into()),
        _ => None,
    }
}

/// Estimate how long the underline should be based on the error message
fn estimate_token_len(msg: &str) -> u32 {
    // Try to extract a quoted token from the message
    if let Some(start) = msg.find('`') {
        if let Some(end) = msg[start+1..].find('`') {
            return (end as u32).max(1);
        }
    }
    if let Some(start) = msg.find('\'') {
        if let Some(end) = msg[start+1..].find('\'') {
            return (end as u32).max(1);
        }
    }
    1
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_err_macro_works() {
        let err = runtime_err!("something went wrong");
        assert!(matches!(err, GravError::Runtime(_)));
        let msg = err.to_string();
        assert!(msg.contains("something went wrong"), "got: {msg}");
    }

    #[test]
    fn runtime_err_with_format() {
        let err = runtime_err!("value is {}", 42);
        assert!(err.to_string().contains("42"));
    }

    #[test]
    fn type_err_macro_works() {
        let err = type_err!("int", "str");
        match &err {
            GravError::Type { expected, got } => {
                assert_eq!(expected, "int");
                assert_eq!(got, "str");
            }
            _ => panic!("expected Type error"),
        }
        let msg = err.to_string();
        assert!(msg.contains("int") && msg.contains("str"), "got: {msg}");
    }

    #[test]
    fn syntax_error_display() {
        let err = GravError::Syntax { line: 5, col: 10, msg: "unexpected token".into() };
        let s = err.to_string();
        assert!(s.contains("5"), "expected line number in: {s}");
        assert!(s.contains("10"), "expected col number in: {s}");
        assert!(s.contains("unexpected token"), "expected message in: {s}");
    }

    #[test]
    fn undefined_var_display() {
        let err = GravError::UndefinedVar("x".into());
        let s = err.to_string();
        assert!(s.contains("x"), "got: {s}");
    }

    #[test]
    fn undefined_fn_display() {
        let err = GravError::UndefinedFn("foo".into());
        let s = err.to_string();
        assert!(s.contains("foo"), "got: {s}");
    }

    #[test]
    fn arity_error_display() {
        let err = GravError::Arity { name: "foo".into(), expected: 2, got: 3 };
        let s = err.to_string();
        assert!(s.contains("foo"), "got: {s}");
        assert!(s.contains("2"), "got: {s}");
        assert!(s.contains("3"), "got: {s}");
    }

    #[test]
    fn bot_error_display() {
        let err = GravError::Bot("connection lost".into());
        let s = err.to_string();
        assert!(s.contains("connection lost"), "got: {s}");
    }

    #[test]
    fn did_you_mean_close_match() {
        let candidates: Vec<String> = vec!["count".into(), "name".into(), "total".into()];
        assert_eq!(did_you_mean("cont", &candidates), Some("count"));
    }

    #[test]
    fn did_you_mean_no_match() {
        let candidates: Vec<String> = vec!["alpha".into(), "beta".into()];
        assert_eq!(did_you_mean("zzzzzzz", &candidates), None);
    }

    #[test]
    fn did_you_mean_exact_not_suggested() {
        let candidates: Vec<String> = vec!["foo".into()];
        // exact match has distance 0, filter requires > 0
        assert_eq!(did_you_mean("foo", &candidates), None);
    }

    #[test]
    fn diagnostic_syntax_error() {
        let err = GravError::Syntax { line: 3, col: 5, msg: "unexpected token".into() };
        let diag = err.to_diagnostic("test.grav", "fn foo() {\n  let x = 42\n  bad token\n}");
        assert!(matches!(diag.kind, DiagKind::Error));
        assert!(diag.code.is_some());
    }

    #[test]
    fn diagnostic_type_error() {
        let err = GravError::Type { expected: "int".into(), got: "str".into() };
        let diag = err.to_diagnostic("test.grav", "");
        assert_eq!(diag.code.as_deref(), Some("G020"));
        assert!(diag.hint.is_some());
    }

    #[test]
    fn diagnostic_display_has_content() {
        let err = GravError::Syntax { line: 1, col: 1, msg: "expected '{'".into() };
        let diag = err.to_diagnostic("test.grav", "fn foo");
        let output = diag.display("test.grav");
        assert!(!output.is_empty());
        assert!(output.contains("error"));
    }
}
