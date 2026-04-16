use crate::error::{GravError, GravResult};

// ─────────────────────────────────────────────────────────────────────────────
// Token types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
    pub col:  u32,
}

impl Token {
    fn new(kind: TokenKind, line: u32, col: u32) -> Self {
        Self { kind, line, col }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Literals ───────────────────────────────
    IntLit(i64),
    FloatLit(f64),
    /// String parts: mix of raw text and `{expr}` interpolation holes
    StrLit(Vec<StrPart>),
    RegexLit { pattern: String, flags: String },
    BoolLit(bool),
    NullLit,

    // ── Identifiers ────────────────────────────
    Ident(String),
    /// Bot slash-command literal:  /start  /my_cmd  (only valid as `on` target)
    SlashCmd(String),

    // ── Keywords ───────────────────────────────
    Let, Fn, On, Flow, State, Emit, Wait,
    Every, At, Guard, Match, If, Else, Elif,
    Return, For, In, While, Break, Continue,
    Run, Pub, Use, Struct, Impl, Env,
    Try, Catch, Callback, Keyboard, Edit, Answer, Test,
    // Vortex-specific statement keywords
    Reply, DeleteMsg, AnswerCallback, SendKeyboard,
    // New feature keywords
    Broadcast,
    Ratelimit,
    Per,
    Cooldown,
    Parallel,
    PerUser,
    PerRoom,
    Wizard,
    Ask,
    Confirm,
    // Feature keywords
    Fsm,
    Permission,
    Require,
    Cache,
    Schedule,
    Assert,
    Rich,
    Reaction,
    Transition(String),  // → state_name (unicode arrow)
    // New feature keywords (features 1-12)
    Hook,
    Before,
    After,
    Plugin,
    Metrics,
    Abtest,
    Variant,
    Federated,
    Idle,
    Stop,
    Template,
    // New feature keywords (features 1-12 new batch)
    Defer,
    Paginate,
    Lang,
    Webhook,
    With,
    // New feature keywords (12 new features)
    Enum,
    Spawn,
    Embed,
    Queue,
    Enqueue,

    // New feature keywords (features 1-12 new batch 2)
    Fire,
    Watch,
    Select,
    Timeout,
    Mock,
    Expect,
    Validate,
    Batch,
    Admin,
    Section,
    Middleware,
    Sandbox,

    // New feature keywords (features N1-N12)
    Intents,
    Intent,
    Unknown,
    Entities,
    Builtin,
    CircuitBreaker,
    WithBreaker,
    Canary,
    Channel,
    DocComment(String),
    Breakpoint,
    Dbg,
    #[allow(dead_code)]
    Install,
    Pkg,
    Multiplatform,
    Migration,
    Scenario,
    Simulate,
    ExpectReply,
    Sends,
    Clicks,
    Contains,
    Equals,
    Matches_,

    // Feature W1-W12 keywords
    Form,
    Field,
    Submit,
    Table,
    Chart,
    Stream,
    Import,
    Typedef,
    Where,
    Finally,
    Websocket,
    Permissions,
    Roles,
    Default,
    Global,

    // ── Architex bridge ──────────────────────
    UiSet,
    UiNavigate,

    // ── Bitwise operators ──────────────────────
    Amp,       // & (single)
    Pipe,      // | (single — not ||, not |>)
    Caret,     // ^
    Tilde,     // ~
    Shl,       // <<
    Shr,       // >>
    AmpEq,    // &=
    PipeEq,   // |=
    CaretEq,  // ^=
    ShlEq,    // <<=
    ShrEq,    // >>=

    // ── Imaginary literal ─────────────────────
    /// `3i`, `2.5i` — imaginary number literal
    ImagLit(f64),

    // ── Types ──────────────────────────────────
    TInt, TFloat, TBool, TStr, TList, TMap, TVoid,

    // ── Arithmetic ─────────────────────────────
    Plus, Minus, Star, Slash, Percent, StarStar,

    // ── Comparison ─────────────────────────────
    EqEq, BangEq, Lt, Gt, LtEq, GtEq,

    // ── Assignment ─────────────────────────────
    Eq, PlusEq, MinusEq, StarEq, SlashEq, PercentEq,

    // ── Logical ────────────────────────────────
    AmpAmp, PipePipe, Bang,

    // ── Special operators ──────────────────────
    PipeGt,          // |>
    Arrow,           // ->
    FatArrow,        // =>
    DotDot,          // ..
    DotDotEq,        // ..=
    DotDotDot,       // ...
    Question,        // ?
    QuestionDot,     // ?.
    QuestionQuestion,// ??
    ColonColon,      // ::

    // ── Delimiters ─────────────────────────────
    LBrace, RBrace,
    LParen, RParen,
    LBracket, RBracket,
    Comma, Semi, Colon, Dot, AtSign,

    Eof,
}

/// One segment of an interpolated string.
#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Lit(String),        // raw text
    Hole(String),       // expression source inside `{…}`
}

// ─────────────────────────────────────────────────────────────────────────────
// Lexer
// ─────────────────────────────────────────────────────────────────────────────

pub struct Lexer<'s> {
    src:  &'s [u8],
    pos:  usize,
    line: u32,
    col:  u32,
}

impl<'s> Lexer<'s> {
    pub fn new(src: &'s str) -> Self {
        Self { src: src.as_bytes(), pos: 0, line: 1, col: 1 }
    }

    pub fn tokenize(mut self) -> GravResult<Vec<Token>> {
        let mut tokens = Vec::with_capacity(256);
        loop {
            let tok = self.next_token()?;
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof { break; }
        }
        Ok(tokens)
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    fn peek(&self) -> u8 { self.src.get(self.pos).copied().unwrap_or(0) }
    fn peek2(&self) -> u8 { self.src.get(self.pos + 1).copied().unwrap_or(0) }

    fn advance(&mut self) -> u8 {
        let c = self.src[self.pos];
        self.pos += 1;
        if c == b'\n' { self.line += 1; self.col = 1; }
        else          { self.col  += 1; }
        c
    }

    fn eat(&mut self, expected: u8) -> bool {
        if self.peek() == expected { self.advance(); true } else { false }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                b' ' | b'\t' | b'\r' | b'\n' => { self.advance(); }
                b'/' if self.peek2() == b'/' && self.src.get(self.pos + 2).copied() == Some(b'/') => {
                    // Doc comment: `///` — don't skip, break out so next_token captures it
                    break;
                }
                b'/' if self.peek2() == b'/' => {
                    while self.peek() != b'\n' && self.peek() != 0 { self.advance(); }
                }
                b'/' if self.peek2() == b'*' => {
                    self.advance(); self.advance(); // consume /*
                    loop {
                        if self.peek() == 0 { break; }
                        if self.peek() == b'*' && self.peek2() == b'/' {
                            self.advance(); self.advance(); break;
                        }
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn err(&self, msg: impl Into<String>) -> GravError {
        GravError::Syntax { line: self.line, col: self.col, msg: msg.into() }
    }

    // ── main dispatch ─────────────────────────────────────────────────────────

    fn next_token(&mut self) -> GravResult<Token> {
        self.skip_whitespace_and_comments();
        let line = self.line;
        let col  = self.col;
        let c = self.peek();

        if c == 0 { return Ok(Token::new(TokenKind::Eof, line, col)); }

        // Handle multi-byte UTF-8 sequences for → (U+2192: E2 86 92)
        if c == 0xE2 && self.src.get(self.pos + 1).copied().unwrap_or(0) == 0x86
            && self.src.get(self.pos + 2).copied().unwrap_or(0) == 0x92 {
            self.pos += 3;
            self.col += 1;
            // Consume whitespace then parse the state name
            self.skip_whitespace_and_comments();
            let start = self.pos;
            while matches!(self.peek(), b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
                self.advance();
            }
            let state_name = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
            return Ok(Token::new(TokenKind::Transition(state_name), line, col));
        }

        let kind = match c {
            // Numbers
            b'0'..=b'9' => self.lex_number()?,

            // Strings
            b'"' => self.lex_string()?,

            // Identifiers / keywords
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident(),

            // Doc comment `///`
            b'/' if self.peek2() == b'/' && self.src.get(self.pos + 2).copied() == Some(b'/') => {
                self.advance(); self.advance(); self.advance(); // consume ///
                // skip optional leading space
                if self.peek() == b' ' { self.advance(); }
                let start = self.pos;
                while self.peek() != b'\n' && self.peek() != 0 { self.advance(); }
                let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
                TokenKind::DocComment(text)
            }
            // Regex  /pattern/flags  OR  /slash_command
            b'/' => self.lex_slash()?,

            // Operators / delimiters
            b'+' => { self.advance(); if self.eat(b'=') { TokenKind::PlusEq    } else { TokenKind::Plus    } }
            b'-' => { self.advance(); if self.eat(b'=') { TokenKind::MinusEq   } else if self.eat(b'>') { TokenKind::Arrow } else { TokenKind::Minus   } }
            b'*' => { self.advance(); if self.eat(b'*') { TokenKind::StarStar  } else if self.eat(b'=') { TokenKind::StarEq } else { TokenKind::Star } }
            b'%' => { self.advance(); if self.eat(b'=') { TokenKind::PercentEq } else { TokenKind::Percent } }
            b'=' => { self.advance(); if self.eat(b'=') { TokenKind::EqEq      } else if self.eat(b'>') { TokenKind::FatArrow } else { TokenKind::Eq } }
            b'!' => { self.advance(); if self.eat(b'=') { TokenKind::BangEq    } else { TokenKind::Bang    } }
            b'<' => { self.advance(); if self.eat(b'=') { TokenKind::LtEq } else if self.eat(b'<') { if self.eat(b'=') { TokenKind::ShlEq } else { TokenKind::Shl } } else { TokenKind::Lt } }
            b'>' => { self.advance(); if self.eat(b'=') { TokenKind::GtEq } else if self.eat(b'>') { if self.eat(b'=') { TokenKind::ShrEq } else { TokenKind::Shr } } else { TokenKind::Gt } }
            b'&' => { self.advance(); if self.eat(b'&') { TokenKind::AmpAmp } else if self.eat(b'=') { TokenKind::AmpEq } else { TokenKind::Amp } }
            b'|' => { self.advance(); if self.eat(b'|') { TokenKind::PipePipe } else if self.eat(b'>') { TokenKind::PipeGt } else if self.eat(b'=') { TokenKind::PipeEq } else { TokenKind::Pipe } }
            b'^' => { self.advance(); if self.eat(b'=') { TokenKind::CaretEq } else { TokenKind::Caret } }
            b'~' => { self.advance(); TokenKind::Tilde }
            b'.' => { self.advance(); if self.eat(b'.') { if self.eat(b'=') { TokenKind::DotDotEq } else if self.eat(b'.') { TokenKind::DotDotDot } else { TokenKind::DotDot } } else { TokenKind::Dot } }
            b':' => { self.advance(); if self.eat(b':') { TokenKind::ColonColon } else { TokenKind::Colon } }
            b'?' => { self.advance(); if self.eat(b'.') { TokenKind::QuestionDot } else if self.eat(b'?') { TokenKind::QuestionQuestion } else { TokenKind::Question } }
            b'@' => { self.advance(); TokenKind::AtSign     }
            b'{' => { self.advance(); TokenKind::LBrace    }
            b'}' => { self.advance(); TokenKind::RBrace    }
            b'(' => { self.advance(); TokenKind::LParen    }
            b')' => { self.advance(); TokenKind::RParen    }
            b'[' => { self.advance(); TokenKind::LBracket  }
            b']' => { self.advance(); TokenKind::RBracket  }
            b',' => { self.advance(); TokenKind::Comma     }
            b';' => { self.advance(); TokenKind::Semi      }

            other => return Err(self.err(format!("unexpected character '{}'", other as char))),
        };

        Ok(Token::new(kind, line, col))
    }

    // ── number literal ────────────────────────────────────────────────────────

    fn lex_number(&mut self) -> GravResult<TokenKind> {
        let start = self.pos;
        while matches!(self.peek(), b'0'..=b'9' | b'_') { self.advance(); }
        let is_float = self.peek() == b'.' && self.peek2() != b'.';
        if is_float {
            self.advance(); // consume '.'
            while matches!(self.peek(), b'0'..=b'9') { self.advance(); }
            if matches!(self.peek(), b'e' | b'E') {
                self.advance();
                if matches!(self.peek(), b'+' | b'-') { self.advance(); }
                while matches!(self.peek(), b'0'..=b'9') { self.advance(); }
            }
            let s = std::str::from_utf8(&self.src[start..self.pos])
                .unwrap()
                .replace('_', "");
            let v: f64 = s.parse().map_err(|_| self.err("invalid float literal"))?;
            // Check for imaginary suffix `i`
            if self.peek() == b'i' && !matches!(self.peek2(), b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
                self.advance();
                return Ok(TokenKind::ImagLit(v));
            }
            Ok(TokenKind::FloatLit(v))
        } else {
            let s = std::str::from_utf8(&self.src[start..self.pos])
                .unwrap()
                .replace('_', "");
            let v: i64 = s.parse().map_err(|_| self.err("invalid integer literal"))?;
            // Check for imaginary suffix `i`
            if self.peek() == b'i' && !matches!(self.peek2(), b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
                self.advance();
                return Ok(TokenKind::ImagLit(v as f64));
            }
            Ok(TokenKind::IntLit(v))
        }
    }

    // ── string literal with `{expr}` interpolation ────────────────────────────

    fn lex_string(&mut self) -> GravResult<TokenKind> {
        self.advance(); // opening "
        let mut parts: Vec<StrPart> = Vec::new();
        let mut buf = String::new();

        loop {
            match self.peek() {
                0 => return Err(self.err("unterminated string")),
                b'"' => { self.advance(); break; }
                b'\\' => {
                    self.advance();
                    let esc = match self.advance() {
                        b'n'  => '\n',
                        b't'  => '\t',
                        b'r'  => '\r',
                        b'\\' => '\\',
                        b'"'  => '"',
                        b'{'  => '{',
                        other => return Err(self.err(format!("unknown escape '\\{}'", other as char))),
                    };
                    buf.push(esc);
                }
                b'{' => {
                    self.advance(); // consume '{'
                    if !buf.is_empty() {
                        parts.push(StrPart::Lit(std::mem::take(&mut buf)));
                    }
                    // Collect until matching '}'
                    let mut depth = 1usize;
                    let mut expr = String::new();
                    loop {
                        match self.peek() {
                            0  => return Err(self.err("unterminated interpolation")),
                            b'{' => { depth += 1; expr.push('{'); self.advance(); }
                            b'}' => {
                                self.advance();
                                depth -= 1;
                                if depth == 0 { break; }
                                expr.push('}');
                            }
                            _ => { expr.push(self.advance() as char); }
                        }
                    }
                    parts.push(StrPart::Hole(expr));
                }
                c => { buf.push(c as char); self.advance(); }
            }
        }

        if !buf.is_empty() { parts.push(StrPart::Lit(buf)); }
        Ok(TokenKind::StrLit(parts))
    }

    // ── identifier / keyword ──────────────────────────────────────────────────

    fn lex_ident(&mut self) -> TokenKind {
        let start = self.pos;
        while matches!(self.peek(), b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
            self.advance();
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        match s {
            "let"      => TokenKind::Let,
            "fn"       => TokenKind::Fn,
            "on"       => TokenKind::On,
            "flow"     => TokenKind::Flow,
            "state"    => TokenKind::State,
            "emit"     => TokenKind::Emit,
            "wait"     => TokenKind::Wait,
            "every"    => TokenKind::Every,
            "at"       => TokenKind::At,
            "guard"    => TokenKind::Guard,
            "match"    => TokenKind::Match,
            "if"       => TokenKind::If,
            "else"     => TokenKind::Else,
            "elif"     => TokenKind::Elif,
            "return"   => TokenKind::Return,
            "for"      => TokenKind::For,
            "in"       => TokenKind::In,
            "while"    => TokenKind::While,
            "break"    => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "run"      => TokenKind::Run,
            "pub"      => TokenKind::Pub,
            "use"      => TokenKind::Use,
            "struct"   => TokenKind::Struct,
            "impl"     => TokenKind::Impl,
            "env"      => TokenKind::Env,
            "try"      => TokenKind::Try,
            "catch"    => TokenKind::Catch,
            "callback" => TokenKind::Callback,
            "keyboard" => TokenKind::Keyboard,
            "edit"     => TokenKind::Edit,
            "answer"   => TokenKind::Answer,
            "test"            => TokenKind::Test,
            "reply"           => TokenKind::Reply,
            "delete_msg"      => TokenKind::DeleteMsg,
            "answer_callback" => TokenKind::AnswerCallback,
            "send_keyboard"   => TokenKind::SendKeyboard,
            "broadcast"       => TokenKind::Broadcast,
            "ratelimit"       => TokenKind::Ratelimit,
            "per"             => TokenKind::Per,
            "cooldown"        => TokenKind::Cooldown,
            "parallel"        => TokenKind::Parallel,
            "per_user"        => TokenKind::PerUser,
            "per_room"        => TokenKind::PerRoom,
            "wizard"          => TokenKind::Wizard,
            "ask"             => TokenKind::Ask,
            "confirm"         => TokenKind::Confirm,
            "fsm"             => TokenKind::Fsm,
            "permission"      => TokenKind::Permission,
            "require"         => TokenKind::Require,
            "cache"           => TokenKind::Cache,
            "schedule"        => TokenKind::Schedule,
            "assert"          => TokenKind::Assert,
            "rich"            => TokenKind::Rich,
            "reaction"        => TokenKind::Reaction,
            "hook"            => TokenKind::Hook,
            "before"          => TokenKind::Before,
            "after"           => TokenKind::After,
            "plugin"          => TokenKind::Plugin,
            "metrics"         => TokenKind::Metrics,
            "abtest"          => TokenKind::Abtest,
            "variant"         => TokenKind::Variant,
            "federated"       => TokenKind::Federated,
            "idle"            => TokenKind::Idle,
            "stop"            => TokenKind::Stop,
            "template"        => TokenKind::Template,
            "defer"           => TokenKind::Defer,
            "paginate"        => TokenKind::Paginate,
            "lang"            => TokenKind::Lang,
            "webhook"         => TokenKind::Webhook,
            "with"            => TokenKind::With,
            "enum"            => TokenKind::Enum,
            "spawn"           => TokenKind::Spawn,
            "embed"           => TokenKind::Embed,
            "queue"           => TokenKind::Queue,
            "enqueue"         => TokenKind::Enqueue,
            "fire"            => TokenKind::Fire,
            "watch"           => TokenKind::Watch,
            "select"          => TokenKind::Select,
            "timeout"         => TokenKind::Timeout,
            "mock"            => TokenKind::Mock,
            "expect"          => TokenKind::Expect,
            "validate"        => TokenKind::Validate,
            "batch"           => TokenKind::Batch,
            "admin"           => TokenKind::Admin,
            "section"         => TokenKind::Section,
            "middleware"       => TokenKind::Middleware,
            "sandbox"         => TokenKind::Sandbox,
            "intents"         => TokenKind::Intents,
            "intent"          => TokenKind::Intent,
            "unknown"         => TokenKind::Unknown,
            "entities"        => TokenKind::Entities,
            "builtin"         => TokenKind::Builtin,
            "circuit_breaker" => TokenKind::CircuitBreaker,
            "with_breaker"    => TokenKind::WithBreaker,
            "canary"          => TokenKind::Canary,
            "channel"         => TokenKind::Channel,
            "breakpoint"      => TokenKind::Breakpoint,
            "debug"           => TokenKind::Dbg,
            "pkg"             => TokenKind::Pkg,
            "multiplatform"   => TokenKind::Multiplatform,
            "migration"       => TokenKind::Migration,
            "scenario"        => TokenKind::Scenario,
            "simulate"        => TokenKind::Simulate,
            "expect_reply"    => TokenKind::ExpectReply,
            "sends"           => TokenKind::Sends,
            "clicks"          => TokenKind::Clicks,
            "contains"        => TokenKind::Contains,
            "equals"          => TokenKind::Equals,
            "matches"         => TokenKind::Matches_,
            "form"            => TokenKind::Form,
            "field"           => TokenKind::Field,
            "submit"          => TokenKind::Submit,
            "table"           => TokenKind::Table,
            "chart"           => TokenKind::Chart,
            "stream"          => TokenKind::Stream,
            "import"          => TokenKind::Import,
            "typedef"         => TokenKind::Typedef,
            "where"           => TokenKind::Where,
            "finally"         => TokenKind::Finally,
            "websocket"       => TokenKind::Websocket,
            "ws"              => TokenKind::Websocket,
            "permissions"     => TokenKind::Permissions,
            "roles"           => TokenKind::Roles,
            "default"         => TokenKind::Default,
            "global"          => TokenKind::Global,
            "ui_set"          => TokenKind::UiSet,
            "ui_navigate"     => TokenKind::UiNavigate,
            "true"            => TokenKind::BoolLit(true),
            "false"    => TokenKind::BoolLit(false),
            "null"     => TokenKind::NullLit,
            "int"      => TokenKind::TInt,
            "float"    => TokenKind::TFloat,
            "bool"     => TokenKind::TBool,
            "str"      => TokenKind::TStr,
            "list"     => TokenKind::TList,
            "map"      => TokenKind::TMap,
            "void"     => TokenKind::TVoid,
            other      => TokenKind::Ident(other.to_string()),
        }
    }

    // ── slash: either /command or /regex/flags ────────────────────────────────

    fn lex_slash(&mut self) -> GravResult<TokenKind> {
        self.advance(); // consume leading '/'

        // /=  →  compound assignment
        if self.peek() == b'=' {
            self.advance();
            return Ok(TokenKind::SlashEq);
        }

        let next = self.peek();

        // Slash command or regex: only when '/' is immediately followed by a
        // letter or underscore (e.g. `/start`, `/hello/i`).
        if next.is_ascii_alphabetic() || next == b'_' {
            // Disambiguate regex vs slash command by presence of closing '/'
            // on the same line.
            let is_regex = {
                let mut tmp = self.pos;
                let mut found_close = false;
                while tmp < self.src.len() {
                    match self.src[tmp] {
                        b'\n' | b'\r' => break,
                        b'\\' => { tmp += 2; continue; }
                        b'/' => { found_close = true; break; }
                        _ => {}
                    }
                    tmp += 1;
                }
                found_close
            };

            if is_regex {
                let mut pattern = String::new();
                loop {
                    match self.peek() {
                        0 | b'\n' => return Err(self.err("unterminated regex")),
                        b'\\' => { self.advance(); let c = self.advance(); pattern.push('\\'); pattern.push(c as char); }
                        b'/' => { self.advance(); break; }
                        c => { pattern.push(c as char); self.advance(); }
                    }
                }
                let mut flags = String::new();
                while matches!(self.peek(), b'a'..=b'z') {
                    flags.push(self.advance() as char);
                }
                return Ok(TokenKind::RegexLit { pattern, flags });
            } else {
                // Slash command:  /start  /my_cmd
                let start = self.pos;
                while matches!(self.peek(), b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') {
                    self.advance();
                }
                let name = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
                return Ok(TokenKind::SlashCmd(name));
            }
        }

        // Non-identifier after '/': treat as division operator.
        Ok(TokenKind::Slash)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Lex source, return only non-Eof token kinds.
    fn lex(src: &str) -> Vec<TokenKind> {
        Lexer::new(src)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Eof))
            .collect()
    }

    // ── 1. Literals ──────────────────────────────────────────────────────────

    #[test]
    fn lex_int() {
        assert_eq!(lex("42"), vec![TokenKind::IntLit(42)]);
    }

    #[test]
    fn lex_negative_int_is_two_tokens() {
        assert_eq!(lex("-7"), vec![TokenKind::Minus, TokenKind::IntLit(7)]);
    }

    #[test]
    fn lex_float() {
        assert_eq!(lex("3.14"), vec![TokenKind::FloatLit(3.14)]);
    }

    #[test]
    fn lex_bool_true() {
        assert_eq!(lex("true"), vec![TokenKind::BoolLit(true)]);
    }

    #[test]
    fn lex_bool_false() {
        assert_eq!(lex("false"), vec![TokenKind::BoolLit(false)]);
    }

    #[test]
    fn lex_null() {
        assert_eq!(lex("null"), vec![TokenKind::NullLit]);
    }

    #[test]
    fn lex_plain_string() {
        let tokens = lex("\"hello\"");
        assert_eq!(tokens, vec![TokenKind::StrLit(vec![StrPart::Lit("hello".into())])]);
    }

    #[test]
    fn lex_float_scientific() {
        assert_eq!(lex("1.5e3"), vec![TokenKind::FloatLit(1500.0)]);
    }

    #[test]
    fn lex_int_with_underscores() {
        assert_eq!(lex("1_000_000"), vec![TokenKind::IntLit(1_000_000)]);
    }

    // ── 2. Keywords ──────────────────────────────────────────────────────────

    #[test]
    fn lex_keywords() {
        assert_eq!(lex("let"), vec![TokenKind::Let]);
        assert_eq!(lex("fn"), vec![TokenKind::Fn]);
        assert_eq!(lex("on"), vec![TokenKind::On]);
        assert_eq!(lex("if"), vec![TokenKind::If]);
        assert_eq!(lex("else"), vec![TokenKind::Else]);
        assert_eq!(lex("return"), vec![TokenKind::Return]);
        assert_eq!(lex("for"), vec![TokenKind::For]);
        assert_eq!(lex("in"), vec![TokenKind::In]);
        assert_eq!(lex("while"), vec![TokenKind::While]);
        assert_eq!(lex("match"), vec![TokenKind::Match]);
        assert_eq!(lex("emit"), vec![TokenKind::Emit]);
        assert_eq!(lex("state"), vec![TokenKind::State]);
        assert_eq!(lex("flow"), vec![TokenKind::Flow]);
        assert_eq!(lex("try"), vec![TokenKind::Try]);
        assert_eq!(lex("catch"), vec![TokenKind::Catch]);
        assert_eq!(lex("finally"), vec![TokenKind::Finally]);
        assert_eq!(lex("import"), vec![TokenKind::Import]);
        assert_eq!(lex("struct"), vec![TokenKind::Struct]);
        assert_eq!(lex("enum"), vec![TokenKind::Enum]);
        assert_eq!(lex("test"), vec![TokenKind::Test]);
        assert_eq!(lex("break"), vec![TokenKind::Break]);
        assert_eq!(lex("continue"), vec![TokenKind::Continue]);
    }

    // ── 3. Operators ─────────────────────────────────────────────────────────

    #[test]
    fn lex_arithmetic() {
        assert_eq!(
            lex("+ - * %"),
            vec![TokenKind::Plus, TokenKind::Minus, TokenKind::Star, TokenKind::Percent]
        );
    }

    #[test]
    fn lex_comparison() {
        assert_eq!(
            lex("== != < > <= >="),
            vec![
                TokenKind::EqEq, TokenKind::BangEq, TokenKind::Lt,
                TokenKind::Gt, TokenKind::LtEq, TokenKind::GtEq,
            ]
        );
    }

    #[test]
    fn lex_logical() {
        assert_eq!(
            lex("&& || !"),
            vec![TokenKind::AmpAmp, TokenKind::PipePipe, TokenKind::Bang]
        );
    }

    #[test]
    fn lex_special_ops() {
        assert_eq!(lex("|>"), vec![TokenKind::PipeGt]);
        assert_eq!(lex("->"), vec![TokenKind::Arrow]);
        assert_eq!(lex("=>"), vec![TokenKind::FatArrow]);
        assert_eq!(lex("??"), vec![TokenKind::QuestionQuestion]);
        assert_eq!(lex("?."), vec![TokenKind::QuestionDot]);
    }

    #[test]
    fn lex_power_operator() {
        assert_eq!(lex("**"), vec![TokenKind::StarStar]);
    }

    #[test]
    fn lex_range_ops() {
        assert_eq!(lex(".."), vec![TokenKind::DotDot]);
        assert_eq!(lex("..="), vec![TokenKind::DotDotEq]);
        assert_eq!(lex("..."), vec![TokenKind::DotDotDot]);
    }

    // ── 4. Delimiters ────────────────────────────────────────────────────────

    #[test]
    fn lex_delimiters() {
        assert_eq!(
            lex("{ } ( ) [ ]"),
            vec![
                TokenKind::LBrace, TokenKind::RBrace,
                TokenKind::LParen, TokenKind::RParen,
                TokenKind::LBracket, TokenKind::RBracket,
            ]
        );
    }

    #[test]
    fn lex_punctuation() {
        assert_eq!(
            lex(", ; : ."),
            vec![TokenKind::Comma, TokenKind::Semi, TokenKind::Colon, TokenKind::Dot]
        );
    }

    // ── 5. Slash commands ────────────────────────────────────────────────────

    #[test]
    fn lex_slash_cmd() {
        let tokens = lex("/start");
        assert_eq!(tokens, vec![TokenKind::SlashCmd("start".into())]);
    }

    #[test]
    fn lex_division_vs_slash_cmd() {
        // `/` followed by non-alpha is division
        assert_eq!(lex("/ 2"), vec![TokenKind::Slash, TokenKind::IntLit(2)]);
    }

    // ── 6. Identifiers ──────────────────────────────────────────────────────

    #[test]
    fn lex_ident() {
        assert_eq!(lex("my_var"), vec![TokenKind::Ident("my_var".into())]);
    }

    #[test]
    fn lex_ident_with_digits() {
        assert_eq!(lex("x42"), vec![TokenKind::Ident("x42".into())]);
    }

    // ── 7. Complex expressions ───────────────────────────────────────────────

    #[test]
    fn lex_let_stmt() {
        let tokens = lex("let x = 42");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0], TokenKind::Let);
        assert_eq!(tokens[1], TokenKind::Ident("x".into()));
        assert_eq!(tokens[2], TokenKind::Eq);
        assert_eq!(tokens[3], TokenKind::IntLit(42));
    }

    #[test]
    fn lex_complex_expr() {
        let tokens = lex("x + y * 2");
        assert_eq!(tokens.len(), 5);
    }

    // ── 8. Comments ──────────────────────────────────────────────────────────

    #[test]
    fn lex_line_comment_skipped() {
        assert_eq!(lex("42 // this is a comment"), vec![TokenKind::IntLit(42)]);
    }

    #[test]
    fn lex_block_comment_skipped() {
        assert_eq!(lex("42 /* block */ 7"), vec![TokenKind::IntLit(42), TokenKind::IntLit(7)]);
    }

    // ── 9. String interpolation ──────────────────────────────────────────────

    #[test]
    fn lex_interpolated_string() {
        let tokens = lex("\"hello {name}\"");
        assert_eq!(tokens.len(), 1);
        if let TokenKind::StrLit(parts) = &tokens[0] {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0], StrPart::Lit("hello ".into()));
            assert_eq!(parts[1], StrPart::Hole("name".into()));
        } else {
            panic!("expected StrLit");
        }
    }

    #[test]
    fn lex_string_escape() {
        let tokens = lex("\"a\\nb\"");
        if let TokenKind::StrLit(parts) = &tokens[0] {
            assert_eq!(parts[0], StrPart::Lit("a\nb".into()));
        } else {
            panic!("expected StrLit");
        }
    }

    // ── 10. Decorator ────────────────────────────────────────────────────────

    #[test]
    fn lex_at_sign() {
        assert_eq!(lex("@"), vec![TokenKind::AtSign]);
    }

    // ── 11. Type tokens ──────────────────────────────────────────────────────

    #[test]
    fn lex_types() {
        assert_eq!(lex("int"), vec![TokenKind::TInt]);
        assert_eq!(lex("float"), vec![TokenKind::TFloat]);
        assert_eq!(lex("bool"), vec![TokenKind::TBool]);
        assert_eq!(lex("str"), vec![TokenKind::TStr]);
        assert_eq!(lex("list"), vec![TokenKind::TList]);
        assert_eq!(lex("map"), vec![TokenKind::TMap]);
        assert_eq!(lex("void"), vec![TokenKind::TVoid]);
    }

    // ── 12. Compound assignment ──────────────────────────────────────────────

    #[test]
    fn lex_compound_assign() {
        assert_eq!(lex("+="), vec![TokenKind::PlusEq]);
        assert_eq!(lex("-="), vec![TokenKind::MinusEq]);
        assert_eq!(lex("*="), vec![TokenKind::StarEq]);
        assert_eq!(lex("/="), vec![TokenKind::SlashEq]);
        assert_eq!(lex("%="), vec![TokenKind::PercentEq]);
    }

    // ── 13. Doc comments ─────────────────────────────────────────────────────

    #[test]
    fn lex_doc_comment() {
        let tokens = lex("/// This is a doc comment");
        assert_eq!(tokens.len(), 1);
        if let TokenKind::DocComment(text) = &tokens[0] {
            assert!(text.contains("doc comment"));
        } else {
            panic!("expected DocComment, got {:?}", tokens[0]);
        }
    }

    // ── 14. Empty input ──────────────────────────────────────────────────────

    #[test]
    fn lex_empty() {
        assert_eq!(lex(""), vec![]);
    }

    #[test]
    fn lex_whitespace_only() {
        assert_eq!(lex("   \n\t  "), vec![]);
    }

    // ── 15. Error cases ──────────────────────────────────────────────────────

    #[test]
    fn lex_unterminated_string() {
        let result = Lexer::new("\"hello").tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn lex_standalone_ampersand_err() {
        // single `&` is an error — must be `&&`
        let result = Lexer::new("&").tokenize();
        assert!(result.is_err());
    }

    // ── 16. Regex literal ────────────────────────────────────────────────────

    #[test]
    fn lex_regex_lit() {
        let tokens = lex("/hello/i");
        assert_eq!(tokens.len(), 1);
        if let TokenKind::RegexLit { pattern, flags } = &tokens[0] {
            assert_eq!(pattern, "hello");
            assert_eq!(flags, "i");
        } else {
            panic!("expected RegexLit");
        }
    }

    // ── 17. Token positions ──────────────────────────────────────────────────

    #[test]
    fn lex_token_positions() {
        let tokens = Lexer::new("let x").tokenize().unwrap();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].col, 1);
        assert_eq!(tokens[1].line, 1);
        assert_eq!(tokens[1].col, 5);
    }

    #[test]
    fn lex_multiline_positions() {
        let tokens = Lexer::new("a\nb").tokenize().unwrap();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[1].line, 2);
    }

    // ── 18. Colons ───────────────────────────────────────────────────────────

    #[test]
    fn lex_colon_colon() {
        assert_eq!(lex("::"), vec![TokenKind::ColonColon]);
    }
}
