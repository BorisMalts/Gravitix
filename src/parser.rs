use crate::ast::*;
use crate::error::{GravError, GravResult};
use crate::lexer::{Token, TokenKind};

// ─────────────────────────────────────────────────────────────────────────────
// Parser — recursive-descent
// ─────────────────────────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<Token>,
    pos:    usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(mut self) -> GravResult<Program> {
        let mut items = Vec::new();
        while !self.at_eof() {
            items.push(self.parse_item()?);
        }
        Ok(Program { items })
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_tok(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() { self.pos += 1; }
        t
    }

    fn at_eof(&self) -> bool { matches!(self.peek(), TokenKind::Eof) }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == kind { self.advance(); true } else { false }
    }

    fn expect(&mut self, kind: &TokenKind) -> GravResult<&Token> {
        if self.peek() == kind {
            Ok(self.advance())
        } else {
            let t = self.peek_tok();
            Err(GravError::Syntax {
                line: t.line, col: t.col,
                msg: format!("expected {:?}, got {:?}", kind, t.kind),
            })
        }
    }

    fn expect_ident(&mut self) -> GravResult<String> {
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.advance();
            Ok(name)
        } else {
            let t = self.peek_tok();
            Err(GravError::Syntax { line: t.line, col: t.col,
                msg: format!("expected identifier, got {:?}", t.kind) })
        }
    }

    fn err(&self, msg: impl Into<String>) -> GravError {
        let t = self.peek_tok();
        GravError::Syntax { line: t.line, col: t.col, msg: msg.into() }
    }

    fn eat_semi(&mut self) { self.eat(&TokenKind::Semi); }

    // ── top-level item ────────────────────────────────────────────────────────

    fn parse_item(&mut self) -> GravResult<Item> {
        // Feature N7: collect doc comments
        let mut doc: Option<String> = None;
        while let TokenKind::DocComment(text) = self.peek().clone() {
            self.advance();
            let d = doc.get_or_insert_with(String::new);
            if !d.is_empty() { d.push('\n'); }
            d.push_str(&text);
        }

        // Feature 1: collect decorators before fn
        if matches!(self.peek(), TokenKind::AtSign) {
            let decorators = self.parse_decorators()?;
            if matches!(self.peek(), TokenKind::Fn) {
                let mut fd = self.parse_fn_with_decorators(decorators)?;
                fd.doc = doc;
                return Ok(Item::FnDef(fd));
            }
            // If no fn follows, treat as error or ignore
            return Err(self.err("decorators must precede a function definition"));
        }
        match self.peek() {
            TokenKind::Fn         => Ok(Item::FnDef(self.parse_fn()?)),
            TokenKind::Template   => Ok(Item::FnDef(self.parse_template()?)),   // Feature 6
            TokenKind::On         => Ok(Item::Handler(self.parse_handler()?)),
            TokenKind::Flow       => Ok(Item::FlowDef(self.parse_flow()?)),
            TokenKind::State      => Ok(Item::StateDef(self.parse_state()?)),
            TokenKind::Every      => Ok(Item::Every(self.parse_every()?)),
            TokenKind::At         => Ok(Item::At(self.parse_at()?)),
            TokenKind::Fsm        => Ok(Item::FsmDef(self.parse_fsm()?)),
            TokenKind::Permission => Ok(Item::PermDef(self.parse_permission()?)),
            TokenKind::Schedule   => Ok(Item::ScheduleDef(self.parse_schedule()?)),
            TokenKind::Hook       => Ok(Item::HookDef(self.parse_hook()?)),       // Feature 3
            TokenKind::Plugin     => Ok(Item::PluginDef(self.parse_plugin()?)),   // Feature 5
            TokenKind::Metrics    => Ok(Item::MetricsDef(self.parse_metrics()?)), // Feature 9
            TokenKind::Abtest     => Ok(Item::AbTestItem(self.parse_abtest()?)),  // Feature 11
            TokenKind::Lang       => Ok(Item::LangDef(self.parse_lang_def()?)),  // Feature 12
            TokenKind::Use    => {
                self.advance(); // use
                // Feature N9: use pkg "name"
                if self.eat(&TokenKind::Pkg) {
                    let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
                        self.advance();
                        parts.into_iter()
                            .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                            .collect::<String>()
                    } else {
                        return Err(self.err("expected package name string after 'use pkg'"));
                    };
                    self.eat_semi();
                    return Ok(Item::UsePkg(name));
                }
                if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    let path = parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>();
                    self.eat_semi();
                    Ok(Item::Use(path))
                } else {
                    Err(self.err("expected file path string after 'use'"))
                }
            }
            TokenKind::Struct => Ok(Item::StructDef(self.parse_struct_def()?)),
            TokenKind::Test   => Ok(Item::TestDef(self.parse_test_def()?)),
            TokenKind::Enum   => Ok(Item::EnumDef(self.parse_enum_def()?)),
            TokenKind::Impl   => Ok(Item::ImplBlock(self.parse_impl_block()?)),
            TokenKind::Queue  => Ok(Item::QueueDef(self.parse_queue_def()?)),
            TokenKind::Watch  => Ok(Item::WatchDef(self.parse_watch_def()?)),
            TokenKind::Admin  => Ok(Item::AdminDef(self.parse_admin_def()?)),
            TokenKind::Middleware => Ok(Item::MiddlewareDef(self.parse_middleware_def()?)),
            TokenKind::Intents => Ok(Item::IntentsDef(self.parse_intents_def()?)),
            TokenKind::Entities => Ok(Item::EntitiesDef(self.parse_entities_def()?)),
            TokenKind::CircuitBreaker => Ok(Item::CircuitBreakerDef(self.parse_circuit_breaker_def()?)),
            TokenKind::Canary => Ok(Item::CanaryDef(self.parse_canary_def()?)),
            TokenKind::Multiplatform => Ok(Item::MultiplatformDef(self.parse_multiplatform_def()?)),
            TokenKind::Migration => Ok(Item::MigrationDef(self.parse_migration_def()?)),
            TokenKind::Webhook => Ok(Item::WebhookDef(self.parse_webhook_def()?)),
            TokenKind::Permissions => Ok(Item::PermissionsDef(self.parse_permissions_def()?)),
            TokenKind::Ratelimit => {
                // Distinguish item-level `ratelimit { ... }` (global def) from inline usage
                // Item-level has `{` after keyword
                if matches!(self.tokens.get(self.pos + 1).map(|t| &t.kind), Some(TokenKind::LBrace)) {
                    Ok(Item::RatelimitDef(self.parse_ratelimit_def()?))
                } else {
                    Ok(Item::Stmt(self.parse_stmt()?))
                }
            }
            TokenKind::Import => {
                self.advance(); // import
                let path = if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>()
                } else {
                    return Err(self.err("expected file path string after 'import'"));
                };
                self.eat_semi();
                Ok(Item::Import(path))
            }
            TokenKind::Typedef => Ok(Item::TypeDefItem(self.parse_typedef_item()?)),
            _                 => Ok(Item::Stmt(self.parse_stmt()?)),
        }
    }

    fn parse_struct_def(&mut self) -> GravResult<StructDef> {
        self.advance(); // struct
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let fname = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;
            fields.push((fname, ty));
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(StructDef { name, fields })
    }

    fn parse_test_def(&mut self) -> GravResult<TestDef> {
        let line = self.peek_tok().line;
        self.advance(); // test
        // Feature N12: test scenario "name" { ... }
        let is_scenario = self.eat(&TokenKind::Scenario);
        // test "name" { body }
        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("expected string name after 'test'"));
        };
        let body = if is_scenario {
            self.parse_scenario_block()?
        } else {
            self.parse_block()?
        };
        Ok(TestDef { name, body, line, is_scenario })
    }

    fn parse_scenario_block(&mut self) -> GravResult<Vec<Stmt>> {
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let stmt = match self.peek() {
                TokenKind::Simulate => {
                    self.advance(); // simulate
                    // simulate user(id) sends/clicks expr
                    // expect ident "user" then (id)
                    let _user_kw = self.expect_ident()?; // "user"
                    self.expect(&TokenKind::LParen)?;
                    let user_id = self.parse_expr()?;
                    self.expect(&TokenKind::RParen)?;
                    let action = if self.eat(&TokenKind::Sends) {
                        SimAction::Sends(self.parse_expr()?)
                    } else if self.eat(&TokenKind::Clicks) {
                        SimAction::Clicks(self.parse_expr()?)
                    } else {
                        return Err(self.err("expected 'sends' or 'clicks'"));
                    };
                    Stmt::Simulate { user_id, action }
                }
                TokenKind::ExpectReply => {
                    self.advance(); // expect_reply
                    let check = if self.eat(&TokenKind::Contains) {
                        ExpectCheck::Contains(self.parse_expr()?)
                    } else if self.eat(&TokenKind::Equals) {
                        ExpectCheck::Equals(self.parse_expr()?)
                    } else if self.eat(&TokenKind::Matches_) {
                        ExpectCheck::Matches(self.parse_expr()?)
                    } else {
                        return Err(self.err("expected 'contains', 'equals', or 'matches'"));
                    };
                    Stmt::ExpectReply { check }
                }
                _ => self.parse_stmt()?,
            };
            stmts.push(stmt);
            self.eat_semi();
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(stmts)
    }

    // ── fn definition ─────────────────────────────────────────────────────────

    fn parse_fn(&mut self) -> GravResult<FnDef> {
        self.parse_fn_with_decorators(vec![])
    }

    fn parse_fn_with_decorators(&mut self, decorators: Vec<crate::ast::Decorator>) -> GravResult<FnDef> {
        let line = self.peek_tok().line;
        self.advance(); // fn
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;
        let ret = if self.eat(&TokenKind::Arrow) { Some(self.parse_type()?) } else { None };
        let body = self.parse_block()?;
        Ok(FnDef { name, params, ret, body, decorators, line, doc: None })
    }

    fn parse_params(&mut self) -> GravResult<Vec<Param>> {
        let mut params = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            let name    = self.expect_ident()?;
            let ty      = if self.eat(&TokenKind::Colon) { Some(self.parse_type()?) } else { None };
            let default = if self.eat(&TokenKind::Eq)    { Some(self.parse_expr()?) } else { None };
            params.push(Param { name, ty, default });
            if !self.eat(&TokenKind::Comma) { break; }
        }
        Ok(params)
    }

    // ── handler ───────────────────────────────────────────────────────────────

    fn parse_handler(&mut self) -> GravResult<Handler> {
        let line = self.peek_tok().line;
        self.advance(); // on
        let trigger = self.parse_trigger()?;
        let guard   = if self.eat(&TokenKind::Guard) { Some(self.parse_expr()?) } else { None };
        let ratelimit = if self.eat(&TokenKind::Ratelimit) {
            Some(self.parse_ratelimit()?)
        } else {
            None
        };
        let require = if self.eat(&TokenKind::Require) {
            Some(self.expect_ident()?)
        } else {
            None
        };
        let body    = self.parse_block()?;
        Ok(Handler { trigger, guard, ratelimit, require, body, line, doc: None })
    }

    fn parse_ratelimit(&mut self) -> GravResult<RateLimit> {
        // Parse: N/Xs  or  N/Xm  or  N/Xh  or  N/min  or  N/sec  or  N/hour
        let count = if let TokenKind::IntLit(n) = self.peek().clone() {
            self.advance(); n as u32
        } else { return Err(self.err("ratelimit: expected count")); };

        self.expect(&TokenKind::Slash)?;

        // Parse window: integer + unit
        let window_ms = if let TokenKind::IntLit(n) = self.peek().clone() {
            self.advance();
            let n = n as u64;
            // expect unit
            match self.peek().clone() {
                TokenKind::Ident(ref s) => {
                    let ms = match s.as_str() {
                        "s" | "sec" | "secs" | "second" | "seconds" => n * 1000,
                        "m" | "min" | "mins" | "minute" | "minutes" => n * 60_000,
                        "h" | "hr" | "hour" | "hours" => n * 3_600_000,
                        _ => return Err(self.err("ratelimit: unknown time unit")),
                    };
                    self.advance();
                    ms
                }
                _ => return Err(self.err("ratelimit: expected time unit (s/m/h)")),
            }
        } else if let TokenKind::Ident(ref s) = self.peek().clone() {
            // e.g. /min /sec /hour directly
            let ms = match s.as_str() {
                "sec" | "secs" => 1_000,
                "min" | "mins" => 60_000,
                "hour" | "hours" => 3_600_000,
                _ => return Err(self.err("ratelimit: expected time window")),
            };
            self.advance();
            ms
        } else {
            return Err(self.err("ratelimit: expected time window"));
        };

        // Parse: per user | per room | per global
        let per = if self.eat(&TokenKind::Per) {
            match self.peek().clone() {
                TokenKind::Ident(ref s) => match s.as_str() {
                    "user"   => { self.advance(); RateLimitScope::User }
                    "room"   => { self.advance(); RateLimitScope::Room }
                    "global" => { self.advance(); RateLimitScope::Global }
                    _ => return Err(self.err("ratelimit: expected 'user', 'room', or 'global'")),
                },
                _ => return Err(self.err("ratelimit: expected scope")),
            }
        } else {
            RateLimitScope::User // default
        };

        // Parse optional: cooldown "message"
        let cooldown = if self.eat(&TokenKind::Cooldown) {
            if let TokenKind::StrLit(parts) = self.peek().clone() {
                self.advance();
                Some(parts.iter().map(|p| match p {
                    crate::lexer::StrPart::Lit(s) => s.clone(),
                    _ => String::new(),
                }).collect())
            } else { None }
        } else { None };

        Ok(RateLimit { count, window_ms, per, cooldown })
    }

    fn parse_trigger(&mut self) -> GravResult<Trigger> {
        match self.peek().clone() {
            TokenKind::SlashCmd(cmd) => { self.advance(); Ok(Trigger::Command(cmd)) }
            TokenKind::Callback => {
                self.advance();
                // `on callback { }` or `on callback ["prefix"] { }`
                let prefix = if matches!(self.peek(), TokenKind::LBracket) {
                    self.advance(); // consume '['
                    let p = if let TokenKind::StrLit(parts) = self.peek().clone() {
                        self.advance();
                        parts.into_iter()
                            .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                            .collect::<String>()
                    } else {
                        return Err(self.err("expected string prefix after 'callback ['"));
                    };
                    self.expect(&TokenKind::RBracket)?;
                    Some(p)
                } else {
                    // Legacy: plain string after callback  on callback "btn_a" { }
                    if let TokenKind::StrLit(parts) = self.peek().clone() {
                        self.advance();
                        let s = parts.into_iter()
                            .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                            .collect::<String>();
                        Some(s)
                    } else {
                        None
                    }
                };
                Ok(Trigger::Callback(prefix))
            }
            TokenKind::Reaction => {
                self.advance();
                // `on reaction "emoji" { }` or `on reaction { }`
                let emoji = if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    let s = parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>();
                    Some(s)
                } else {
                    None
                };
                Ok(Trigger::Reaction(emoji))
            }
            TokenKind::Idle => {
                self.advance(); // idle
                self.expect(&TokenKind::LParen)?;
                let n = if let TokenKind::IntLit(n) = self.peek().clone() {
                    self.advance(); n as u64
                } else { return Err(self.err("idle: expected duration number")); };
                // parse unit (s/m/h)
                let ms = match self.peek().clone() {
                    TokenKind::Ident(ref unit) => {
                        let ms = match unit.as_str() {
                            "s" | "sec" | "secs" => n * 1_000,
                            "m" | "min" | "mins" => n * 60_000,
                            "h" | "hr" | "hour" | "hours" => n * 3_600_000,
                            _ => return Err(self.err("idle: unknown time unit")),
                        };
                        self.advance();
                        ms
                    }
                    _ => n * 60_000, // default: minutes
                };
                self.expect(&TokenKind::RParen)?;
                Ok(Trigger::Idle(ms))
            }
            // Feature 10: `on webhook "/path" { }`
            TokenKind::Webhook => {
                self.advance();
                let path = if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>()
                } else {
                    return Err(self.err("expected string path after 'webhook'"));
                };
                Ok(Trigger::Webhook(path))
            }
            TokenKind::Intent => {
                self.advance();
                if self.eat(&TokenKind::Unknown) {
                    return Ok(Trigger::IntentUnknown);
                }
                if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    let name = parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>();
                    return Ok(Trigger::Intent(name));
                }
                if let TokenKind::Ident(ref name) = self.peek().clone() {
                    if name == "unknown" { self.advance(); return Ok(Trigger::IntentUnknown); }
                    let n = name.clone(); self.advance();
                    return Ok(Trigger::Intent(n));
                }
                return Err(self.err("expected intent name or 'unknown'"));
            }
            TokenKind::Ident(ref s) => {
                let s = s.clone();
                self.advance();
                match s.as_str() {
                    "msg"                   => Ok(Trigger::AnyMsg),
                    "join"                  => Ok(Trigger::Join),
                    "leave"                 => Ok(Trigger::Leave),
                    "edited" | "edited_msg" => Ok(Trigger::EditedMsg),
                    "any"                   => Ok(Trigger::Any),
                    "error"                 => Ok(Trigger::Error),
                    // Feature 2: Media triggers
                    "file"                  => Ok(Trigger::File),
                    "image"                 => Ok(Trigger::Image),
                    "voice_msg"             => Ok(Trigger::VoiceMsg),
                    // Feature 4: Mention / DM
                    "mention"               => Ok(Trigger::Mention),
                    "dm"                    => Ok(Trigger::Dm),
                    // Feature 10: poll_vote / thread / forward triggers
                    "poll_vote"             => Ok(Trigger::PollVote),
                    "thread"                => Ok(Trigger::Thread),
                    "forward"               => Ok(Trigger::Forward),
                    "event"                 => {
                        // on event "name" { }
                        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
                            self.advance();
                            parts.into_iter()
                                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                                .collect::<String>()
                        } else {
                            return Err(self.err("expected event name string"));
                        };
                        Ok(Trigger::Event(name))
                    }
                    "intent"                => {
                        // on intent "name" { } or on intent unknown { }
                        if self.eat(&TokenKind::Unknown) {
                            Ok(Trigger::IntentUnknown)
                        } else if let TokenKind::StrLit(parts) = self.peek().clone() {
                            self.advance();
                            let name = parts.into_iter()
                                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                                .collect::<String>();
                            Ok(Trigger::Intent(name))
                        } else if let TokenKind::Ident(ref name) = self.peek().clone() {
                            if name == "unknown" {
                                self.advance();
                                Ok(Trigger::IntentUnknown)
                            } else {
                                let n = name.clone();
                                self.advance();
                                Ok(Trigger::Intent(n))
                            }
                        } else {
                            Err(self.err("expected intent name or 'unknown'"))
                        }
                    }
                    _                       => Err(self.err(format!("unknown trigger '{s}'"))),
                }
            }
            _ => Err(self.err("expected trigger (e.g. /start, msg, callback, join, leave)")),
        }
    }

    // ── flow ──────────────────────────────────────────────────────────────────

    fn parse_flow(&mut self) -> GravResult<FlowDef> {
        let line = self.peek_tok().line;
        self.advance(); // flow
        let name = self.expect_ident()?;
        let body = self.parse_block()?;
        Ok(FlowDef { name, body, line, doc: None })
    }

    // ── state ─────────────────────────────────────────────────────────────────

    fn parse_state(&mut self) -> GravResult<StateDef> {
        self.advance(); // state
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let scope = match self.peek() {
                TokenKind::PerUser  => { self.advance(); StateScope::PerUser }
                TokenKind::PerRoom  => { self.advance(); StateScope::PerRoom }
                TokenKind::Ident(s) if s == "global" => { self.advance(); StateScope::Global }
                _ => StateScope::Global,
            };
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let default = if self.eat(&TokenKind::Eq) { Some(self.parse_expr()?) } else { None };
            fields.push(StateField { name, ty, default, scope });
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(StateDef { fields })
    }

    // ── every / at ────────────────────────────────────────────────────────────

    fn parse_every(&mut self) -> GravResult<EveryDef> {
        self.advance(); // every

        // Feature 10: Human-readable cron syntax
        // `every monday at 9:00 { }`, `every weekday at 18:00 { }`, `every 1st of month at 10:00 { }`
        if let TokenKind::Ident(ref s) = self.peek().clone() {
            let day_name = s.clone();
            match day_name.as_str() {
                "monday" | "tuesday" | "wednesday" | "thursday" | "friday"
                | "saturday" | "sunday" | "weekday" | "weekend" => {
                    self.advance(); // day name
                    // expect `at`
                    self.expect(&TokenKind::At)?;
                    let (hour, minute) = self.parse_time_literal()?;
                    let cron = match day_name.as_str() {
                        "monday"    => format!("{minute} {hour} * * 1"),
                        "tuesday"   => format!("{minute} {hour} * * 2"),
                        "wednesday" => format!("{minute} {hour} * * 3"),
                        "thursday"  => format!("{minute} {hour} * * 4"),
                        "friday"    => format!("{minute} {hour} * * 5"),
                        "saturday"  => format!("{minute} {hour} * * 6"),
                        "sunday"    => format!("{minute} {hour} * * 0"),
                        "weekday"   => format!("{minute} {hour} * * 1-5"),
                        "weekend"   => format!("{minute} {hour} * * 0,6"),
                        _ => unreachable!(),
                    };
                    let body = self.parse_block()?;
                    // Store as ScheduleDef via an EveryDef wrapper — convert cron to ScheduleDef
                    // Actually, return EveryDef with a 1-day interval as a placeholder
                    // The cron is stored for the scheduler to handle
                    let _ = cron; // Simplified: use EveryDef with daily interval
                    return Ok(EveryDef { amount: 1, unit: TimeUnit::Day, body });
                }
                _ => {}
            }
        }

        // Check for ordinal: `every 1st of month at 10:00`
        if let TokenKind::IntLit(n) = self.peek().clone() {
            // Peek ahead to see if this is `N` + ident like "st"/"nd"/"rd"/"th"
            let saved_pos = self.pos;
            self.advance();
            if let TokenKind::Ident(ref suffix) = self.peek().clone() {
                if matches!(suffix.as_str(), "st" | "nd" | "rd" | "th") {
                    self.advance(); // suffix
                    // expect `of month at HH:MM`
                    if let TokenKind::Ident(ref of_kw) = self.peek().clone() {
                        if of_kw == "of" {
                            self.advance(); // of
                            if let TokenKind::Ident(ref month_kw) = self.peek().clone() {
                                if month_kw == "month" {
                                    self.advance(); // month
                                    self.expect(&TokenKind::At)?;
                                    let (_hour, _minute) = self.parse_time_literal()?;
                                    let _ = n; // day of month
                                    let body = self.parse_block()?;
                                    return Ok(EveryDef { amount: 1, unit: TimeUnit::Day, body });
                                }
                            }
                        }
                    }
                }
            }
            // Backtrack
            self.pos = saved_pos;
        }

        let amount = if let TokenKind::IntLit(n) = self.peek().clone() {
            self.advance(); n as u64
        } else { return Err(self.err("expected duration number after 'every'")); };
        let unit = self.parse_time_unit()?;
        let body = self.parse_block()?;
        Ok(EveryDef { amount, unit, body })
    }

    /// Parse a time literal like `9:00` or `18:30` (as int:int tokens)
    fn parse_time_literal(&mut self) -> GravResult<(u64, u64)> {
        // Could be a string "9:00" or individual tokens
        if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            let s: String = parts.iter().filter_map(|p| {
                if let crate::lexer::StrPart::Lit(s) = p { Some(s.clone()) } else { None }
            }).collect();
            let parts: Vec<&str> = s.split(':').collect();
            let h = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            let m = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            return Ok((h, m));
        }
        // Try int:int
        let h = if let TokenKind::IntLit(n) = self.peek().clone() {
            self.advance(); n as u64
        } else { return Err(self.err("expected time (HH:MM)")); };
        if self.eat(&TokenKind::Colon) {
            let m = if let TokenKind::IntLit(n) = self.peek().clone() {
                self.advance(); n as u64
            } else { 0 };
            Ok((h, m))
        } else {
            Ok((h, 0))
        }
    }

    fn parse_at(&mut self) -> GravResult<AtDef> {
        self.advance(); // at
        let time = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.iter().map(|p| match p {
                crate::lexer::StrPart::Lit(s) => s.clone(),
                crate::lexer::StrPart::Hole(_) => String::new(),
            }).collect()
        } else { return Err(self.err("expected time string after 'at'")); };
        let body = self.parse_block()?;
        Ok(AtDef { time, body })
    }

    fn parse_time_unit(&mut self) -> GravResult<TimeUnit> {
        if let TokenKind::Ident(s) = self.peek().clone() {
            self.advance();
            Ok(match s.as_str() {
                "s" | "sec" | "secs" | "second" | "seconds" => TimeUnit::Sec,
                "m" | "min" | "mins" | "minute" | "minutes" => TimeUnit::Min,
                "h" | "hr"  | "hrs"  | "hour"   | "hours"   => TimeUnit::Hour,
                "d" | "day" | "days"                         => TimeUnit::Day,
                _ => return Err(self.err(format!("unknown time unit '{s}'"))),
            })
        } else {
            Err(self.err("expected time unit (s, min, h, d)"))
        }
    }

    // ── block ─────────────────────────────────────────────────────────────────

    fn parse_block(&mut self) -> GravResult<Vec<Stmt>> {
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(stmts)
    }

    // ── statement ─────────────────────────────────────────────────────────────

    fn parse_stmt(&mut self) -> GravResult<Stmt> {
        let stmt = match self.peek() {
            TokenKind::Let      => self.parse_let_or_destruct()?,
            TokenKind::Emit     => self.parse_emit()?,
            TokenKind::Return   => { self.advance(); let e = if matches!(self.peek(), TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof) { None } else { Some(self.parse_expr()?) }; Stmt::Return(e) }
            TokenKind::Break    => { self.advance(); Stmt::Break }
            TokenKind::Continue => { self.advance(); Stmt::Continue }
            TokenKind::If       => self.parse_if()?,
            TokenKind::While    => self.parse_while()?,
            TokenKind::For      => self.parse_for()?,
            TokenKind::Match    => self.parse_match()?,
            TokenKind::Run      => self.parse_run()?,
            TokenKind::Try            => self.parse_try_catch()?,
            TokenKind::Keyboard       => self.parse_keyboard()?,
            TokenKind::Edit           => self.parse_edit_msg()?,
            TokenKind::Answer         => self.parse_answer()?,
            TokenKind::Reply          => {
                self.advance();
                let reply_to = self.parse_expr()?;
                let text     = self.parse_expr()?;
                Stmt::Reply { reply_to, text }
            }
            TokenKind::DeleteMsg      => {
                self.advance();
                let msg_id = self.parse_expr()?;
                Stmt::DeleteMsg(msg_id)
            }
            TokenKind::AnswerCallback => {
                self.advance();
                let text = if matches!(self.peek(), TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                Stmt::AnswerCallback(text)
            }
            TokenKind::SendKeyboard   => {
                self.advance();
                let text    = self.parse_expr()?;
                let buttons = self.parse_expr()?;
                Stmt::SendKeyboard { text, buttons }
            }
            TokenKind::Wizard    => self.parse_wizard()?,
            TokenKind::Assert    => self.parse_assert()?,
            TokenKind::Stop      => { self.advance(); Stmt::Stop }
            TokenKind::Federated => self.parse_federated_emit()?,
            TokenKind::Abtest    => Stmt::AbTest(self.parse_abtest()?),
            TokenKind::Defer     => self.parse_defer()?,
            TokenKind::Paginate  => self.parse_paginate()?,
            TokenKind::Spawn     => self.parse_spawn()?,
            TokenKind::Embed     => self.parse_embed_stmt()?,
            TokenKind::Enqueue   => self.parse_enqueue()?,
            TokenKind::Fire      => self.parse_fire()?,
            TokenKind::Select    => self.parse_select()?,
            TokenKind::Mock      => self.parse_mock()?,
            TokenKind::Validate  => self.parse_validate()?,
            TokenKind::Batch     => self.parse_batch()?,
            TokenKind::Breakpoint => { self.advance(); Stmt::Breakpoint }
            TokenKind::Dbg       => self.parse_debug_stmt()?,
            TokenKind::Simulate  => self.parse_simulate_stmt()?,
            TokenKind::ExpectReply => self.parse_expect_reply_stmt()?,
            TokenKind::Table     => self.parse_table_stmt()?,
            TokenKind::Chart     => self.parse_chart_stmt()?,
            TokenKind::Stream    => self.parse_stream_stmt()?,
            TokenKind::Transition(_) => {
                if let TokenKind::Transition(state_name) = self.advance().kind.clone() {
                    Stmt::Transition(state_name)
                } else { unreachable!() }
            }
            _                   => {
                let expr = self.parse_expr()?;
                // Check for assignment
                if self.eat(&TokenKind::Eq) {
                    let value = self.parse_expr()?;
                    self.eat_semi();
                    return Ok(Stmt::Assign { target: expr, value });
                }
                // Compound assignments
                let op = match self.peek() {
                    TokenKind::PlusEq    => Some(BinOp::Add),
                    TokenKind::MinusEq   => Some(BinOp::Sub),
                    TokenKind::StarEq    => Some(BinOp::Mul),
                    TokenKind::SlashEq   => Some(BinOp::Div),
                    TokenKind::PercentEq => Some(BinOp::Rem),
                    _ => None,
                };
                if let Some(op) = op {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.eat_semi();
                    return Ok(Stmt::CompoundAssign { target: expr, op, value });
                }
                Stmt::Expr(expr)
            }
        };
        self.eat_semi();
        Ok(stmt)
    }

    fn parse_let_or_destruct(&mut self) -> GravResult<Stmt> {
        self.advance(); // let

        // Feature 1: `let {name, age} = expr` — map destructuring
        if matches!(self.peek(), TokenKind::LBrace) {
            self.advance(); // {
            let mut fields = Vec::new();
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                fields.push(self.expect_ident()?);
                if !self.eat(&TokenKind::Comma) { break; }
            }
            self.expect(&TokenKind::RBrace)?;
            self.expect(&TokenKind::Eq)?;
            let value = self.parse_expr()?;
            return Ok(Stmt::LetDestructMap { fields, value });
        }

        // Feature 1: `let [first, second, ...rest] = expr` — list destructuring
        if matches!(self.peek(), TokenKind::LBracket) {
            self.advance(); // [
            let mut items = Vec::new();
            let mut rest = None;
            while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                if matches!(self.peek(), TokenKind::DotDotDot) {
                    self.advance(); // ...
                    rest = Some(self.expect_ident()?);
                    break;
                }
                items.push(self.expect_ident()?);
                if !self.eat(&TokenKind::Comma) { break; }
            }
            self.expect(&TokenKind::RBracket)?;
            self.expect(&TokenKind::Eq)?;
            let value = self.parse_expr()?;
            return Ok(Stmt::LetDestructList { items, rest, value });
        }

        // Normal let
        let name = self.expect_ident()?;
        let ty   = if self.eat(&TokenKind::Colon) { Some(self.parse_type()?) } else { None };
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        Ok(Stmt::Let { name, ty, value })
    }

    fn parse_emit(&mut self) -> GravResult<Stmt> {
        self.advance(); // emit
        // `emit broadcast expr` → EmitBroadcast
        if matches!(self.peek(), TokenKind::Broadcast) {
            self.advance();
            let msg = self.parse_expr()?;
            return Ok(Stmt::EmitBroadcast(msg));
        }
        // `emit to <room_id>, <msg>` → EmitTo
        if matches!(self.peek(), TokenKind::Ident(s) if s == "to") {
            self.advance(); // to
            let target = self.parse_expr()?;
            self.expect(&TokenKind::Comma)?;
            let msg = self.parse_expr()?;
            return Ok(Stmt::EmitTo { target, msg });
        }
        // `emit rich { key: val, … }` → EmitRich
        if matches!(self.peek(), TokenKind::Rich) {
            self.advance(); // rich
            self.expect(&TokenKind::LBrace)?;
            let mut fields = Vec::new();
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                fields.push((key, val));
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace)?;
            return Ok(Stmt::EmitRich { fields });
        }
        let msg = self.parse_expr()?;
        Ok(Stmt::Emit(msg))
    }

    fn parse_if(&mut self) -> GravResult<Stmt> {
        self.advance(); // if
        let cond = self.parse_expr()?;
        let then = self.parse_block()?;
        let mut elif = Vec::new();
        let mut else_ = None;
        loop {
            if matches!(self.peek(), TokenKind::Elif) {
                self.advance();
                let c = self.parse_expr()?;
                let b = self.parse_block()?;
                elif.push((c, b));
            } else if self.eat(&TokenKind::Else) {
                else_ = Some(self.parse_block()?);
                break;
            } else { break; }
        }
        Ok(Stmt::If { cond, then, elif, else_ })
    }

    fn parse_while(&mut self) -> GravResult<Stmt> {
        self.advance(); // while
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_for(&mut self) -> GravResult<Stmt> {
        self.advance(); // for
        let var  = self.expect_ident()?;
        self.expect(&TokenKind::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::For { var, iter, body })
    }

    fn parse_match(&mut self) -> GravResult<Stmt> {
        self.advance(); // match
        let subject = self.parse_expr()?;
        self.expect(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let pattern = self.parse_pattern()?;
            self.expect(&TokenKind::FatArrow)?;
            let body = if matches!(self.peek(), TokenKind::LBrace) {
                self.parse_block()?
            } else {
                let s = self.parse_stmt()?;
                vec![s]
            };
            self.eat(&TokenKind::Comma);
            arms.push(MatchArm { pattern, body });
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Stmt::Match { subject, arms })
    }

    fn parse_pattern(&mut self) -> GravResult<Pattern> {
        match self.peek().clone() {
            TokenKind::Ident(s) if s == "_" => { self.advance(); Ok(Pattern::Wild) }
            TokenKind::RegexLit { pattern, flags } => {
                self.advance(); Ok(Pattern::Regex { pattern, flags })
            }
            // `name @ pattern` — binding pattern
            // Also: `Ok(var)` / `Err(var)` / `EnumName.Variant(bindings)`
            TokenKind::Ident(name) => {
                self.advance();
                // Ok(var) / Err(var) — Result pattern
                if (name == "Ok" || name == "Err") && matches!(self.peek(), TokenKind::LParen) {
                    self.advance(); // (
                    let mut bindings = Vec::new();
                    while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                        bindings.push(self.expect_ident()?);
                        if !self.eat(&TokenKind::Comma) { break; }
                    }
                    self.expect(&TokenKind::RParen)?;
                    return Ok(Pattern::EnumDestruct {
                        enum_name: "Result".to_string(),
                        variant:   name,
                        bindings,
                    });
                }
                // EnumName.Variant or EnumName.Variant(bindings)
                if matches!(self.peek(), TokenKind::Dot) && name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    self.advance(); // .
                    let variant = self.expect_ident()?;
                    if matches!(self.peek(), TokenKind::LParen) {
                        self.advance(); // (
                        let mut bindings = Vec::new();
                        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                            bindings.push(self.expect_ident()?);
                            if !self.eat(&TokenKind::Comma) { break; }
                        }
                        self.expect(&TokenKind::RParen)?;
                        return Ok(Pattern::EnumDestruct {
                            enum_name: name,
                            variant,
                            bindings,
                        });
                    } else {
                        // Unit variant pattern: EnumName.Variant
                        return Ok(Pattern::EnumDestruct {
                            enum_name: name,
                            variant,
                            bindings: vec![],
                        });
                    }
                }
                if self.eat(&TokenKind::AtSign) {
                    let inner = self.parse_pattern()?;
                    Ok(Pattern::Bind { name, inner: Box::new(inner) })
                } else {
                    // treat identifier as a literal expression (variable reference)
                    Ok(Pattern::Lit(Expr::Var(name)))
                }
            }
            _ => Ok(Pattern::Lit(self.parse_expr()?)),
        }
    }

    fn parse_run(&mut self) -> GravResult<Stmt> {
        self.advance(); // run
        if let TokenKind::Fsm = self.peek() {
            self.advance(); // fsm
            let name = self.expect_ident()?;
            return Ok(Stmt::RunFsm(name));
        }
        if let TokenKind::Flow = self.peek() { self.advance(); }
        let name = self.expect_ident()?;
        Ok(Stmt::RunFlow(name))
    }

    fn parse_try_catch(&mut self) -> GravResult<Stmt> {
        self.advance(); // try
        let try_body = self.parse_block()?;
        self.expect(&TokenKind::Catch)?;
        let err_name = self.expect_ident()?;
        let catch_body = self.parse_block()?;
        let finally_body = if self.eat(&TokenKind::Finally) {
            self.parse_block()?
        } else {
            vec![]
        };
        Ok(Stmt::TryCatch { try_body, err_name, catch_body, finally_body })
    }

    fn parse_keyboard(&mut self) -> GravResult<Stmt> {
        self.advance(); // keyboard
        let text = self.parse_expr()?;
        self.expect(&TokenKind::Comma)?;
        let buttons = self.parse_expr()?;
        self.eat_semi();
        Ok(Stmt::SendKeyboard { text, buttons })
    }

    fn parse_edit_msg(&mut self) -> GravResult<Stmt> {
        self.advance(); // edit
        let msg_id = self.parse_expr()?;
        self.expect(&TokenKind::Comma)?;
        let text = self.parse_expr()?;
        self.eat_semi();
        Ok(Stmt::EditMsg { msg_id, text })
    }

    fn parse_answer(&mut self) -> GravResult<Stmt> {
        self.advance(); // answer
        let text = if matches!(self.peek(), TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.eat_semi();
        Ok(Stmt::AnswerCallback(text))
    }

    fn parse_wizard(&mut self) -> GravResult<Stmt> {
        self.advance(); // wizard
        self.expect(&TokenKind::Arrow)?; // →
        let output_var = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let mut steps = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek().clone() {
                TokenKind::Ask => {
                    self.advance();
                    let prompt = self.parse_expr()?;
                    self.expect(&TokenKind::Arrow)?;
                    let var = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    let ty = self.parse_type()?;
                    let validate = if matches!(self.peek(), TokenKind::Ident(s) if s == "validate") {
                        self.advance();
                        let body = self.parse_block()?;
                        Some(Expr::Lambda { params: vec![], body })
                    } else { None };
                    steps.push(WizardStep { prompt, var, ty, validate, is_confirm: false });
                }
                TokenKind::Confirm => {
                    self.advance();
                    let prompt = self.parse_expr()?;
                    steps.push(WizardStep {
                        prompt,
                        var:        "__confirm".into(),
                        ty:         TypeExpr::Bool,
                        validate:   None,
                        is_confirm: true,
                    });
                }
                _ => break,
            }
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Stmt::Wizard { output_var, steps })
    }

    fn parse_assert(&mut self) -> GravResult<Stmt> {
        self.advance(); // assert
        let cond = self.parse_expr()?;
        let msg = if self.eat(&TokenKind::Comma) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Stmt::Assert { cond, msg })
    }

    // ── FSM definition ────────────────────────────────────────────────────────

    fn parse_fsm(&mut self) -> GravResult<FsmDef> {
        self.advance(); // fsm
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;

        // initial: state_name
        let mut initial = String::new();
        let mut states  = Vec::new();

        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            if let TokenKind::Ident(s) = self.peek().clone() {
                if s == "initial" {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    initial = self.expect_ident()?;
                    self.eat_semi();
                    continue;
                }
            }
            if matches!(self.peek(), TokenKind::State) {
                self.advance(); // state
                let state_name = self.expect_ident()?;
                self.expect(&TokenKind::LBrace)?;
                let mut on_enter = Vec::new();
                let mut on_leave = Vec::new();
                let mut handlers = Vec::new();

                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    // on_enter { ... }
                    if let TokenKind::Ident(s) = self.peek().clone() {
                        if s == "on_enter" {
                            self.advance();
                            on_enter = self.parse_block()?;
                            continue;
                        }
                        if s == "on_leave" {
                            self.advance();
                            on_leave = self.parse_block()?;
                            continue;
                        }
                    }
                    // on <trigger> { ... }
                    if matches!(self.peek(), TokenKind::On) {
                        self.advance(); // on
                        let trigger = match self.peek().clone() {
                            TokenKind::SlashCmd(cmd) => { self.advance(); crate::ast::FsmTrigger::Command(cmd) }
                            TokenKind::Ident(s) => {
                                self.advance();
                                match s.as_str() {
                                    "msg" => crate::ast::FsmTrigger::AnyMsg,
                                    other => crate::ast::FsmTrigger::Other(other.to_string()),
                                }
                            }
                            _ => return Err(self.err("expected FSM trigger")),
                        };
                        let body = self.parse_block()?;
                        handlers.push(crate::ast::FsmHandler { trigger, body });
                        continue;
                    }
                    return Err(self.err("unexpected token in fsm state"));
                }
                self.expect(&TokenKind::RBrace)?;
                states.push(crate::ast::FsmState { name: state_name, on_enter, on_leave, handlers });
                continue;
            }
            return Err(self.err("expected 'initial:' or 'state' in fsm"));
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(FsmDef { name, initial, states })
    }

    // ── permission definition ─────────────────────────────────────────────────

    fn parse_permission(&mut self) -> GravResult<PermDef> {
        self.advance(); // permission
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let cond = self.parse_expr()?;
        self.eat_semi();
        self.expect(&TokenKind::RBrace)?;
        Ok(PermDef { name, cond })
    }

    // ── schedule definition ───────────────────────────────────────────────────

    fn parse_schedule(&mut self) -> GravResult<ScheduleDef> {
        self.advance(); // schedule
        let cron = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.iter().map(|p| match p {
                crate::lexer::StrPart::Lit(s) => s.clone(),
                crate::lexer::StrPart::Hole(_) => String::new(),
            }).collect()
        } else {
            return Err(self.err("expected cron string after 'schedule'"));
        };
        let body = self.parse_block()?;
        Ok(ScheduleDef { cron, body })
    }

    // ── Feature 3: hook before/after msg { body } ────────────────────────────

    fn parse_hook(&mut self) -> GravResult<HookDef> {
        self.advance(); // hook
        let when = match self.peek() {
            TokenKind::Before => { self.advance(); HookWhen::Before }
            TokenKind::After  => { self.advance(); HookWhen::After  }
            TokenKind::Ident(s) if s == "before" => { self.advance(); HookWhen::Before }
            TokenKind::Ident(s) if s == "after"  => { self.advance(); HookWhen::After  }
            _ => return Err(self.err("expected 'before' or 'after' after 'hook'")),
        };
        // optional trigger name (msg, etc.) — just consume it
        match self.peek() {
            TokenKind::Ident(s) if s == "msg" => { self.advance(); }
            TokenKind::Ident(_) => { self.advance(); }
            _ => {}
        }
        let body = self.parse_block()?;
        Ok(HookDef { when, body })
    }

    // ── Feature 5: plugin "name" { key: expr, … } ────────────────────────────

    fn parse_plugin(&mut self) -> GravResult<PluginDef> {
        self.advance(); // plugin
        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("expected plugin name string"));
        };
        let mut config = Vec::new();
        if self.eat(&TokenKind::LBrace) {
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                config.push((key, val));
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace)?;
        }
        Ok(PluginDef { name, config })
    }

    // ── Feature 6: template name(params) { body } ────────────────────────────
    // Desugared to FnDef

    fn parse_template(&mut self) -> GravResult<FnDef> {
        let line = self.peek_tok().line;
        self.advance(); // template
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;
        let body = self.parse_block()?;
        Ok(FnDef { name, params, ret: None, body, decorators: vec![], line, doc: None })
    }

    // ── Feature 7: federated emit "room@node" expr ───────────────────────────

    fn parse_federated_emit(&mut self) -> GravResult<Stmt> {
        self.advance(); // federated
        self.expect(&TokenKind::Emit)?;
        let target = self.parse_expr()?;
        let msg    = self.parse_expr()?;
        Ok(Stmt::FederatedEmit { target, msg })
    }

    // ── Feature 9: metrics { counter x, gauge y, … } ─────────────────────────

    fn parse_metrics(&mut self) -> GravResult<MetricsDef> {
        self.advance(); // metrics
        self.expect(&TokenKind::LBrace)?;
        let mut defs = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let kind = match self.peek().clone() {
                TokenKind::Ident(ref s) => match s.as_str() {
                    "counter"   => { self.advance(); MetricKind::Counter }
                    "gauge"     => { self.advance(); MetricKind::Gauge }
                    "histogram" => { self.advance(); MetricKind::Histogram }
                    _ => return Err(self.err("metrics: expected counter/gauge/histogram")),
                },
                _ => return Err(self.err("metrics: expected counter/gauge/histogram")),
            };
            let name = self.expect_ident()?;
            defs.push(MetricDef { kind, name });
            self.eat(&TokenKind::Comma);
            self.eat_semi();
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(MetricsDef { defs })
    }

    // ── Feature 11: abtest "name" { variant A { } variant B { } } ────────────

    fn parse_abtest(&mut self) -> GravResult<AbTestDef> {
        self.advance(); // abtest
        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("abtest: expected name string"));
        };
        self.expect(&TokenKind::LBrace)?;
        let mut variant_a = Vec::new();
        let mut variant_b = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            // expect: variant A { } or variant B { }
            match self.peek() {
                TokenKind::Variant => { self.advance(); }
                TokenKind::Ident(s) if s == "variant" => { self.advance(); }
                _ => return Err(self.err("abtest: expected 'variant'")),
            }
            let label = self.expect_ident()?;
            let body = self.parse_block()?;
            match label.as_str() {
                "A" | "a" => variant_a = body,
                "B" | "b" => variant_b = body,
                _ => return Err(self.err(format!("abtest: unknown variant '{label}' (use A or B)"))),
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(AbTestDef { name, variant_a, variant_b })
    }

    // ── Feature 4: defer { body } ──────────────────────────────────────────────

    fn parse_defer(&mut self) -> GravResult<Stmt> {
        self.advance(); // defer
        let body = self.parse_block()?;
        Ok(Stmt::Defer { body })
    }

    // ── Feature 11: paginate(items, page_size) [with { ... }] ────────────────

    fn parse_paginate(&mut self) -> GravResult<Stmt> {
        self.advance(); // paginate
        self.expect(&TokenKind::LParen)?;
        let items = self.parse_expr()?;
        self.expect(&TokenKind::Comma)?;
        let page_size = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        let (mut format_fn, mut title) = (None, None);
        if self.eat(&TokenKind::With) {
            self.expect(&TokenKind::LBrace)?;
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                match key.as_str() {
                    "format" => format_fn = Some(val),
                    "title"  => title = Some(val),
                    _        => {}
                }
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace)?;
        }
        Ok(Stmt::Paginate { items, page_size, format_fn, title })
    }

    // ── Feature 12: lang { ru: { ... }, en: { ... } } ────────────────────────

    fn parse_lang_def(&mut self) -> GravResult<LangDef> {
        self.advance(); // lang
        self.expect(&TokenKind::LBrace)?;
        let mut locales = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let code = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            self.expect(&TokenKind::LBrace)?;
            let mut kv = Vec::new();
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                kv.push((key, val));
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace)?;
            locales.push((code, kv));
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(LangDef { locales })
    }

    // ── expression hierarchy (Pratt-style) ───────────────────────────────────
    //   or → and → compare → add → mul → unary → postfix → primary

    pub(crate) fn parse_expr(&mut self) -> GravResult<Expr> {
        let lhs = self.parse_null_coalesce()?;
        // Range expressions: `a..b` (exclusive) and `a..=b` (inclusive)
        if self.eat(&TokenKind::DotDotEq) {
            let rhs = self.parse_null_coalesce()?;
            return Ok(Expr::Binary { op: BinOp::RangeIn, lhs: Box::new(lhs), rhs: Box::new(rhs) });
        }
        if self.eat(&TokenKind::DotDot) {
            let rhs = self.parse_null_coalesce()?;
            return Ok(Expr::Binary { op: BinOp::RangeEx, lhs: Box::new(lhs), rhs: Box::new(rhs) });
        }
        Ok(lhs)
    }

    /// Feature 3: `??` — null coalescing, precedence between `or` and top-level
    fn parse_null_coalesce(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_or()?;
        while self.eat(&TokenKind::QuestionQuestion) {
            let rhs = self.parse_or()?;
            lhs = Expr::Binary { op: BinOp::NullCoalesce, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    /// Public alias for `parse_expr` — used by interpreter for dynamic eval
    pub fn parse_expr_pub(&mut self) -> GravResult<Expr> {
        self.parse_expr()
    }

    fn parse_or(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_and()?;
        while self.eat(&TokenKind::PipePipe) {
            let rhs = self.parse_and()?;
            lhs = Expr::Binary { op: BinOp::Or, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_bit_or()?;
        while self.eat(&TokenKind::AmpAmp) {
            let rhs = self.parse_bit_or()?;
            lhs = Expr::Binary { op: BinOp::And, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_bit_or(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_bit_xor()?;
        while self.eat(&TokenKind::Pipe) {
            let rhs = self.parse_bit_xor()?;
            lhs = Expr::Binary { op: BinOp::BitOr, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_bit_xor(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_bit_and()?;
        while self.eat(&TokenKind::Caret) {
            let rhs = self.parse_bit_and()?;
            lhs = Expr::Binary { op: BinOp::BitXor, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_bit_and(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_compare()?;
        while self.eat(&TokenKind::Amp) {
            let rhs = self.parse_compare()?;
            lhs = Expr::Binary { op: BinOp::BitAnd, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_compare(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_shift()?;
        loop {
            let op = match self.peek() {
                TokenKind::EqEq  => BinOp::Eq,
                TokenKind::BangEq => BinOp::Ne,
                TokenKind::Lt    => BinOp::Lt,
                TokenKind::Gt    => BinOp::Gt,
                TokenKind::LtEq  => BinOp::Le,
                TokenKind::GtEq  => BinOp::Ge,
                _                => break,
            };
            self.advance();
            let rhs = self.parse_add()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_shift(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek() {
                TokenKind::Shl => BinOp::Shl,
                TokenKind::Shr => BinOp::Shr,
                _              => break,
            };
            self.advance();
            let rhs = self.parse_add()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus  => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _                => break,
            };
            self.advance();
            let rhs = self.parse_mul()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> GravResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star    => BinOp::Mul,
                TokenKind::Slash   => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                TokenKind::StarStar => BinOp::Pow,
                TokenKind::PipeGt  => {
                    self.advance();
                    let fn_name = self.expect_ident()?;
                    // Feature 11: check for `?` after ident for try propagation
                    let try_ = self.eat(&TokenKind::Question);
                    // Support `|> fn(extra_args)` — lhs is prepended as first arg
                    if self.eat(&TokenKind::LParen) {
                        let mut args = vec![lhs];
                        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                            args.push(self.parse_expr()?);
                            if !self.eat(&TokenKind::Comma) { break; }
                        }
                        self.expect(&TokenKind::RParen)?;
                        lhs = Expr::Call { name: fn_name, args };
                        if try_ { lhs = Expr::Try(Box::new(lhs)); }
                    } else {
                        lhs = Expr::Pipe { lhs: Box::new(lhs), fn_name, try_ };
                    }
                    continue;
                }
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> GravResult<Expr> {
        match self.peek() {
            TokenKind::Bang  => { self.advance(); Ok(Expr::Unary { op: UnaryOp::Not, expr: Box::new(self.parse_unary()?) }) }
            TokenKind::Minus => { self.advance(); Ok(Expr::Unary { op: UnaryOp::Neg, expr: Box::new(self.parse_unary()?) }) }
            _                => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> GravResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    if self.eat(&TokenKind::LParen) {
                        let args = self.parse_args()?;
                        self.expect(&TokenKind::RParen)?;
                        expr = Expr::Method { object: Box::new(expr), method: field, args };
                    } else {
                        expr = Expr::Field { object: Box::new(expr), field };
                    }
                }
                // Feature 6: `?` — try/unwrap Result
                TokenKind::Question => {
                    self.advance();
                    expr = Expr::Try(Box::new(expr));
                }
                // Feature 3: `?.` — optional chaining
                TokenKind::QuestionDot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    if self.eat(&TokenKind::LParen) {
                        let args = self.parse_args()?;
                        self.expect(&TokenKind::RParen)?;
                        expr = Expr::OptionalMethod { object: Box::new(expr), method: field, args };
                    } else {
                        expr = Expr::OptionalField { object: Box::new(expr), field };
                    }
                }
                TokenKind::LBracket => {
                    self.advance();
                    // Detect slice: expr[start:end] or expr[:end] or expr[start:]
                    if self.eat(&TokenKind::Colon) {
                        // expr[:end]
                        let end = if matches!(self.peek(), TokenKind::RBracket) { None }
                                  else { Some(Box::new(self.parse_expr()?)) };
                        self.expect(&TokenKind::RBracket)?;
                        expr = Expr::Slice { object: Box::new(expr), start: None, end };
                    } else {
                        let idx = self.parse_expr()?;
                        if self.eat(&TokenKind::Colon) {
                            // expr[start:end] or expr[start:]
                            let end = if matches!(self.peek(), TokenKind::RBracket) { None }
                                      else { Some(Box::new(self.parse_expr()?)) };
                            self.expect(&TokenKind::RBracket)?;
                            expr = Expr::Slice { object: Box::new(expr), start: Some(Box::new(idx)), end };
                        } else {
                            self.expect(&TokenKind::RBracket)?;
                            expr = Expr::Index { object: Box::new(expr), index: Box::new(idx) };
                        }
                    }
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> GravResult<Expr> {
        match self.peek().clone() {
            TokenKind::IntLit(n)      => { self.advance(); Ok(Expr::Int(n)) }
            TokenKind::FloatLit(f)    => { self.advance(); Ok(Expr::Float(f)) }
            TokenKind::BoolLit(b)     => { self.advance(); Ok(Expr::Bool(b)) }
            TokenKind::NullLit        => { self.advance(); Ok(Expr::Null) }
            TokenKind::StrLit(parts)  => { self.advance(); Ok(Expr::Str(parts)) }
            TokenKind::Wait           => {
                self.advance();
                // `wait callback` vs `wait` (wait msg)
                if matches!(self.peek(), TokenKind::Callback) {
                    self.advance();
                    Ok(Expr::WaitCallback)
                } else {
                    Ok(Expr::Wait)
                }
            }
            TokenKind::State          => { self.advance(); Ok(Expr::StateRef) }
            TokenKind::Ident(ref s) if s == "ctx"   => { self.advance(); Ok(Expr::Ctx) }
            TokenKind::Ident(ref s) if s == "state" => { self.advance(); Ok(Expr::StateRef) }
            TokenKind::Env            => {
                self.advance();
                self.expect(&TokenKind::LParen)?;
                let key = if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    parts.iter().map(|p| match p {
                        crate::lexer::StrPart::Lit(s) => s.clone(),
                        _ => String::new(),
                    }).collect()
                } else { return Err(self.err("expected string key in env()")); };
                self.expect(&TokenKind::RParen)?;
                Ok(Expr::EnvVar(key))
            }
            // Type keywords used as conversion functions: int(x), float(x), str(x), bool(x)
            TokenKind::TInt   => { self.advance(); let args = self.parse_call_args()?; Ok(Expr::Call { name: "int".into(),   args }) }
            TokenKind::TFloat => { self.advance(); let args = self.parse_call_args()?; Ok(Expr::Call { name: "float".into(), args }) }
            TokenKind::TStr   => { self.advance(); let args = self.parse_call_args()?; Ok(Expr::Call { name: "str".into(),   args }) }
            TokenKind::TBool  => { self.advance(); let args = self.parse_call_args()?; Ok(Expr::Call { name: "bool".into(),  args }) }

            TokenKind::Fn => {
                // Anonymous function / closure:  fn(params) { body }
                self.advance();
                self.expect(&TokenKind::LParen)?;
                let params = self.parse_params()?;
                self.expect(&TokenKind::RParen)?;
                let body = self.parse_block()?;
                Ok(Expr::Lambda { params, body })
            }

            TokenKind::Ident(name) => {
                self.advance();
                if self.eat(&TokenKind::LParen) {
                    let args = self.parse_args()?;
                    self.expect(&TokenKind::RParen)?;
                    Ok(Expr::Call { name, args })
                } else if matches!(self.peek(), TokenKind::LBrace) {
                    // Struct literal: TypeName { field: val, … }
                    // Only parse as struct literal if not at statement level
                    // (heuristic: uppercase first char)
                    if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                        self.advance(); // {
                        let mut fields = Vec::new();
                        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                            let fname = self.expect_ident()?;
                            self.expect(&TokenKind::Colon)?;
                            let val = self.parse_expr()?;
                            fields.push((fname, val));
                            self.eat(&TokenKind::Comma);
                        }
                        self.expect(&TokenKind::RBrace)?;
                        Ok(Expr::StructLit { type_name: name, fields })
                    } else {
                        Ok(Expr::Var(name))
                    }
                } else {
                    Ok(Expr::Var(name))
                }
            }
            TokenKind::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(e)
            }
            TokenKind::LBracket => {
                self.advance();
                // Feature 2: list comprehension  [expr for var in iter] or [expr for var in iter if cond]
                if matches!(self.peek(), TokenKind::RBracket) {
                    self.advance();
                    return Ok(Expr::List(vec![]));
                }
                let first_expr = self.parse_expr()?;
                if matches!(self.peek(), TokenKind::For) {
                    // List comprehension
                    self.advance(); // for
                    let var = self.expect_ident()?;
                    self.expect(&TokenKind::In)?;
                    let iter = self.parse_expr()?;
                    let cond = if matches!(self.peek(), TokenKind::If) {
                        self.advance();
                        Some(Box::new(self.parse_expr()?))
                    } else { None };
                    self.expect(&TokenKind::RBracket)?;
                    return Ok(Expr::ListComp {
                        expr: Box::new(first_expr),
                        var,
                        iter: Box::new(iter),
                        cond,
                    });
                }
                // Normal list literal
                let mut elems = vec![first_expr];
                while self.eat(&TokenKind::Comma) {
                    if matches!(self.peek(), TokenKind::RBracket) { break; }
                    elems.push(self.parse_expr()?);
                }
                self.expect(&TokenKind::RBracket)?;
                Ok(Expr::List(elems))
            }
            TokenKind::LBrace => {
                self.advance();
                let mut pairs = Vec::new();
                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    let k = self.parse_expr()?;
                    self.expect(&TokenKind::Colon)?;
                    let v = self.parse_expr()?;
                    pairs.push((k, v));
                    if !self.eat(&TokenKind::Comma) { break; }
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Expr::Map(pairs))
            }
            TokenKind::Parallel => {
                self.advance();
                self.expect(&TokenKind::LBrace)?;
                let mut exprs = Vec::new();
                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    exprs.push(self.parse_expr()?);
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Expr::Parallel(exprs))
            }
            TokenKind::Cache => {
                self.advance(); // cache
                self.expect(&TokenKind::LParen)?;
                let key = self.parse_expr()?;
                self.expect(&TokenKind::Comma)?;
                let ttl = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                let body = self.parse_block()?;
                Ok(Expr::Cache { key: Box::new(key), ttl_secs: Box::new(ttl), body })
            }
            TokenKind::Sandbox => {
                self.advance(); // sandbox
                self.expect(&TokenKind::LBrace)?;
                let mut config = Vec::new();
                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    let val = self.parse_expr()?;
                    config.push((key, val));
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Expr::Sandbox { config })
            }
            TokenKind::Expect => {
                self.advance(); // expect
                let args = self.parse_call_args()?;
                Ok(Expr::Call { name: "expect".into(), args })
            }
            TokenKind::WithBreaker => {
                self.advance(); // with_breaker
                let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>()
                } else {
                    return Err(self.err("expected breaker name string"));
                };
                let body = self.parse_block()?;
                Ok(Expr::WithBreaker { name, body })
            }
            TokenKind::Channel => {
                self.advance(); // channel
                let args = self.parse_call_args()?;
                Ok(Expr::Call { name: "channel".into(), args })
            }
            TokenKind::Form => {
                self.advance(); // form
                self.expect(&TokenKind::LBrace)?;
                let mut fields = Vec::new();
                let mut submit: Option<String> = None;
                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    if self.eat(&TokenKind::Submit) {
                        // submit "label"
                        let label = if let TokenKind::StrLit(parts) = self.peek().clone() {
                            self.advance();
                            parts.into_iter()
                                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                                .collect::<String>()
                        } else { "Submit".to_string() };
                        submit = Some(label);
                    } else if self.eat(&TokenKind::Field) {
                        // field "name" kind [required]
                        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
                            self.advance();
                            parts.into_iter()
                                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                                .collect::<String>()
                        } else { self.expect_ident()? };
                        let kind_name = self.expect_ident()?;
                        let kind = match kind_name.as_str() {
                            "textarea" => crate::ast::FormFieldKind::Textarea,
                            "number"   => crate::ast::FormFieldKind::Number,
                            "email"    => crate::ast::FormFieldKind::Email,
                            "phone"    => crate::ast::FormFieldKind::Phone,
                            _          => crate::ast::FormFieldKind::Text,
                        };
                        let required = if let TokenKind::Ident(ref s) = self.peek() {
                            if s == "required" { self.advance(); true } else { false }
                        } else { false };
                        fields.push(crate::ast::FormField { name, kind, required });
                    } else {
                        self.advance(); // skip unknown tokens inside form
                    }
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Expr::Form { fields, submit })
            }
            TokenKind::Websocket => {
                self.advance(); // websocket
                let url = self.parse_expr()?;
                let mut config = Vec::new();
                if self.eat(&TokenKind::LBrace) {
                    while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                        let key = self.expect_ident()?;
                        self.expect(&TokenKind::Colon)?;
                        let val = self.parse_expr()?;
                        config.push((key, val));
                        self.eat(&TokenKind::Comma);
                    }
                    self.expect(&TokenKind::RBrace)?;
                }
                Ok(Expr::WebSocket { url: Box::new(url), config })
            }
            other => Err(self.err(format!("unexpected token {:?}", other))),
        }
    }

    /// Parse `(args...)` — used for type-as-function calls like `int(x)`
    fn parse_call_args(&mut self) -> GravResult<Vec<Expr>> {
        self.expect(&TokenKind::LParen)?;
        let args = self.parse_args()?;
        self.expect(&TokenKind::RParen)?;
        Ok(args)
    }

    fn parse_args(&mut self) -> GravResult<Vec<Expr>> {
        let mut args = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            args.push(self.parse_expr()?);
            if !self.eat(&TokenKind::Comma) { break; }
        }
        Ok(args)
    }

    // ── type expressions ──────────────────────────────────────────────────────

    // ── enum definition ────────────────────────────────────────────────────────

    fn parse_enum_def(&mut self) -> GravResult<EnumDef> {
        self.advance(); // enum
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let vname = self.expect_ident()?;
            let fields = if self.eat(&TokenKind::LParen) {
                let mut f = Vec::new();
                while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                    f.push(self.parse_type()?);
                    if !self.eat(&TokenKind::Comma) { break; }
                }
                self.expect(&TokenKind::RParen)?;
                f
            } else {
                vec![]
            };
            variants.push(EnumVariant { name: vname, fields });
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(EnumDef { name, variants })
    }

    // ── impl block ───────────────────────────────────────────────────────────

    fn parse_impl_block(&mut self) -> GravResult<ImplBlock> {
        self.advance(); // impl
        let type_name = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let mut methods = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            if matches!(self.peek(), TokenKind::Fn) {
                methods.push(self.parse_fn()?);
            } else {
                return Err(self.err("expected 'fn' in impl block"));
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(ImplBlock { type_name, methods })
    }

    // ── queue definition ─────────────────────────────────────────────────────

    fn parse_queue_def(&mut self) -> GravResult<QueueDef> {
        self.advance(); // queue
        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("expected queue name string"));
        };
        let mut config = Vec::new();
        if self.eat(&TokenKind::LBrace) {
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                config.push((key, val));
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace)?;
        }
        Ok(QueueDef { name, config })
    }

    // ── spawn { body } ──────────────────────────────────────────────────────

    fn parse_spawn(&mut self) -> GravResult<Stmt> {
        self.advance(); // spawn
        let body = self.parse_block()?;
        Ok(Stmt::Spawn { body })
    }

    // ── embed { key: val, ... } ─────────────────────────────────────────────

    fn parse_embed_stmt(&mut self) -> GravResult<Stmt> {
        self.advance(); // embed
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let val = self.parse_expr()?;
            fields.push((key, val));
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Stmt::Embed { fields })
    }

    // ── enqueue "queue_name" { body } ───────────────────────────────────────

    fn parse_enqueue(&mut self) -> GravResult<Stmt> {
        self.advance(); // enqueue
        let queue_name = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("expected queue name string after 'enqueue'"));
        };
        let body = self.parse_block()?;
        Ok(Stmt::Enqueue { queue_name, body })
    }

    // ── Feature 1: decorators ────────────────────────────────────────────────

    fn parse_decorators(&mut self) -> GravResult<Vec<crate::ast::Decorator>> {
        let mut decorators = Vec::new();
        while matches!(self.peek(), TokenKind::AtSign) {
            self.advance(); // @
            let name = self.expect_ident()?;
            let args = if self.eat(&TokenKind::LParen) {
                let a = self.parse_args()?;
                self.expect(&TokenKind::RParen)?;
                a
            } else {
                vec![]
            };
            decorators.push(crate::ast::Decorator { name, args });
        }
        Ok(decorators)
    }

    // ── Feature 2: fire "event" data ─────────────────────────────────────────

    fn parse_fire(&mut self) -> GravResult<Stmt> {
        self.advance(); // fire
        let event = self.parse_expr()?;
        let data = if matches!(self.peek(), TokenKind::Semi | TokenKind::RBrace | TokenKind::Eof) {
            Expr::Null
        } else {
            self.parse_expr()?
        };
        Ok(Stmt::Fire { event, data })
    }

    // ── Feature 3: watch state.field { body } ──────────────────────────────

    fn parse_watch_def(&mut self) -> GravResult<WatchDef> {
        self.advance(); // watch
        // expect state.field
        self.expect(&TokenKind::State)?;
        self.expect(&TokenKind::Dot)?;
        let field = self.expect_ident()?;
        let body = self.parse_block()?;
        Ok(WatchDef { field, body })
    }

    // ── Feature 4: select { arms } ──────────────────────────────────────────

    fn parse_select(&mut self) -> GravResult<Stmt> {
        self.advance(); // select
        self.expect(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let kind = if matches!(self.peek(), TokenKind::Wait) {
                self.advance(); // wait
                if matches!(self.peek(), TokenKind::Callback) {
                    self.advance();
                    let prefix = if let TokenKind::StrLit(parts) = self.peek().clone() {
                        self.advance();
                        Some(parts.into_iter()
                            .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                            .collect::<String>())
                    } else { None };
                    crate::ast::SelectKind::WaitCallback(prefix)
                } else {
                    // wait msg
                    if let TokenKind::Ident(ref s) = self.peek().clone() {
                        if s == "msg" { self.advance(); }
                    }
                    crate::ast::SelectKind::WaitMsg
                }
            } else if matches!(self.peek(), TokenKind::Timeout) {
                self.advance(); // timeout
                let n = if let TokenKind::IntLit(n) = self.peek().clone() {
                    self.advance(); n as u64
                } else { 60 };
                // parse optional unit
                let ms = if let TokenKind::Ident(ref u) = self.peek().clone() {
                    let ms = match u.as_str() {
                        "s" | "sec" | "secs" => n * 1000,
                        "m" | "min" | "mins" => n * 60_000,
                        "ms" => n,
                        _ => n * 1000,
                    };
                    self.advance();
                    ms
                } else { n * 1000 };
                crate::ast::SelectKind::Timeout(ms)
            } else {
                return Err(self.err("select: expected 'wait msg', 'wait callback', or 'timeout'"));
            };

            let guard = if self.eat(&TokenKind::Guard) {
                Some(self.parse_expr()?)
            } else { None };

            self.expect(&TokenKind::FatArrow)?;
            let body = self.parse_block()?;
            arms.push(crate::ast::SelectArm { kind, guard, body });
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Stmt::Select { arms })
    }

    // ── Feature 5: mock target { body } ──────────────────────────────────────

    fn parse_mock(&mut self) -> GravResult<Stmt> {
        self.advance(); // mock
        // Parse target: could be `http.get` or just `name`
        let mut target = self.expect_ident()?;
        while self.eat(&TokenKind::Dot) {
            let part = self.expect_ident()?;
            target = format!("{target}.{part}");
        }
        let body = self.parse_block()?;
        Ok(Stmt::Mock { target, body })
    }

    // ── Feature 6: validate expr as kind or { body } ────────────────────────

    fn parse_validate(&mut self) -> GravResult<Stmt> {
        self.advance(); // validate
        let value = self.parse_expr()?;
        // expect `as`
        if let TokenKind::Ident(ref s) = self.peek().clone() {
            if s == "as" { self.advance(); }
            else { return Err(self.err("validate: expected 'as'")); }
        } else {
            return Err(self.err("validate: expected 'as'"));
        }
        let kind_name = self.expect_ident()?;
        let mut kind_args = Vec::new();
        // Optional args like range(1, 100) or len(1, 255)
        if matches!(self.peek(), TokenKind::LParen) || (kind_name == "range" || kind_name == "len") {
            // Already consumed name; check for parens on a second call identifier
        }
        // Check for `range(...)` or `len(...)` as a follow-up
        if let TokenKind::Ident(ref s) = self.peek().clone() {
            if matches!(s.as_str(), "range" | "len") {
                let _extra_name = s.clone();
                self.advance();
                if self.eat(&TokenKind::LParen) {
                    while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                        kind_args.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) { break; }
                    }
                    self.expect(&TokenKind::RParen)?;
                }
            }
        }
        // Also check if the kind_name itself has parens
        if self.eat(&TokenKind::LParen) {
            while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                kind_args.push(self.parse_expr()?);
                if !self.eat(&TokenKind::Comma) { break; }
            }
            self.expect(&TokenKind::RParen)?;
        }
        let kind = crate::ast::ValidateKind { name: kind_name, args: kind_args };
        // expect `or`
        let or_body = if let TokenKind::Ident(ref s) = self.peek().clone() {
            if s == "or" {
                self.advance();
                self.parse_block()?
            } else { vec![] }
        } else { vec![] };
        Ok(Stmt::Validate { value, kind, or_body })
    }

    // ── Feature 8: batch { body } ────────────────────────────────────────────

    fn parse_batch(&mut self) -> GravResult<Stmt> {
        self.advance(); // batch
        let body = self.parse_block()?;
        Ok(Stmt::Batch { body })
    }

    // ── Feature 9: admin { ... } ─────────────────────────────────────────────

    fn parse_admin_def(&mut self) -> GravResult<AdminDef> {
        self.advance(); // admin
        self.expect(&TokenKind::LBrace)?;
        let mut config = Vec::new();
        let mut sections = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            if matches!(self.peek(), TokenKind::Section) {
                self.advance(); // section
                let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>()
                } else {
                    self.expect_ident()?
                };
                self.expect(&TokenKind::LBrace)?;
                let mut sec_config = Vec::new();
                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    let key = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    let val = self.parse_expr()?;
                    sec_config.push((key, val));
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBrace)?;
                sections.push(crate::ast::AdminSection { name, config: sec_config });
            } else {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                config.push((key, val));
                self.eat(&TokenKind::Comma);
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(AdminDef { config, sections })
    }

    // ── Feature 11: middleware name(params) { body } ──────────────────────────

    fn parse_middleware_def(&mut self) -> GravResult<MiddlewareDef> {
        self.advance(); // middleware
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;
        let body = self.parse_block()?;
        Ok(MiddlewareDef { name, params, body })
    }

    // ── type expressions ──────────────────────────────────────────────────────

    fn parse_type(&mut self) -> GravResult<TypeExpr> {
        let base = match self.peek().clone() {
            TokenKind::TInt   => { self.advance(); TypeExpr::Int }
            TokenKind::TFloat => { self.advance(); TypeExpr::Float }
            TokenKind::TBool  => { self.advance(); TypeExpr::Bool }
            TokenKind::TStr   => { self.advance(); TypeExpr::Str }
            TokenKind::TVoid  => { self.advance(); TypeExpr::Void }
            TokenKind::TList  => {
                self.advance();
                if self.eat(&TokenKind::Lt) {
                    let inner = self.parse_type()?;
                    self.expect(&TokenKind::Gt)?;
                    TypeExpr::List(Box::new(inner))
                } else { TypeExpr::List(Box::new(TypeExpr::Any)) }
            }
            TokenKind::TMap => {
                self.advance();
                if self.eat(&TokenKind::Lt) {
                    let k = self.parse_type()?;
                    self.expect(&TokenKind::Comma)?;
                    let v = self.parse_type()?;
                    self.expect(&TokenKind::Gt)?;
                    TypeExpr::Map(Box::new(k), Box::new(v))
                } else { TypeExpr::Map(Box::new(TypeExpr::Str), Box::new(TypeExpr::Any)) }
            }
            TokenKind::Ident(name) => {
                self.advance();
                match name.as_str() {
                    "Result" => TypeExpr::Result,
                    "any"    => TypeExpr::Any,
                    _        => TypeExpr::Named(name),
                }
            }
            _ => return Err(self.err("expected type")),
        };
        if self.eat(&TokenKind::Question) {
            Ok(TypeExpr::Optional(Box::new(base)))
        } else { Ok(base) }
    }

    // ── Feature N1: intents { name: [phrases], ... } ─────────────────────────

    fn parse_intents_def(&mut self) -> GravResult<IntentsDef> {
        self.advance(); // intents
        self.expect(&TokenKind::LBrace)?;
        let mut intents = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            self.expect(&TokenKind::LBracket)?;
            let mut phrases = Vec::new();
            while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    let s: String = parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect();
                    phrases.push(s);
                } else {
                    return Err(self.err("expected string in intent phrases"));
                }
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBracket)?;
            intents.push((name, phrases));
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(IntentsDef { intents })
    }

    // ── Feature N2: entities { name: builtin|[list], ... } ───────────────────

    fn parse_entities_def(&mut self) -> GravResult<EntitiesDef> {
        self.advance(); // entities
        self.expect(&TokenKind::LBrace)?;
        let mut entities = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let kind = if self.eat(&TokenKind::Builtin) {
                EntityKind::Builtin
            } else if let TokenKind::Ident(ref s) = self.peek().clone() {
                if s == "builtin" { self.advance(); EntityKind::Builtin }
                else { return Err(self.err("expected 'builtin' or list")); }
            } else if matches!(self.peek(), TokenKind::LBracket) {
                self.advance(); // [
                let mut items = Vec::new();
                while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                    if let TokenKind::StrLit(parts) = self.peek().clone() {
                        self.advance();
                        let s: String = parts.into_iter()
                            .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                            .collect();
                        items.push(s);
                    } else {
                        return Err(self.err("expected string in entity list"));
                    }
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBracket)?;
                EntityKind::List(items)
            } else {
                return Err(self.err("expected 'builtin' or [list]"));
            };
            entities.push(EntityDef { name, kind });
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(EntitiesDef { entities })
    }

    // ── Feature N3: circuit_breaker "name" { config } ────────────────────────

    fn parse_circuit_breaker_def(&mut self) -> GravResult<CircuitBreakerDef> {
        self.advance(); // circuit_breaker
        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("expected breaker name string"));
        };
        let mut config = Vec::new();
        if self.eat(&TokenKind::LBrace) {
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                config.push((key, val));
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace)?;
        }
        Ok(CircuitBreakerDef { name, config })
    }

    // ── Feature N5: canary "name" { percent: N, on trigger { } } ────────────

    fn parse_canary_def(&mut self) -> GravResult<CanaryDef> {
        self.advance(); // canary
        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("expected canary name string"));
        };
        self.expect(&TokenKind::LBrace)?;
        let mut percent: u8 = 10;
        let mut handlers = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            if let TokenKind::Ident(ref s) = self.peek().clone() {
                if s == "percent" {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    if let TokenKind::IntLit(n) = self.peek().clone() {
                        self.advance();
                        percent = n as u8;
                    }
                    self.eat(&TokenKind::Comma);
                    continue;
                }
            }
            if matches!(self.peek(), TokenKind::On) {
                handlers.push(self.parse_handler()?);
                continue;
            }
            return Err(self.err("expected 'percent:' or 'on' in canary"));
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(CanaryDef { name, percent, handlers })
    }

    // ── Feature N10: multiplatform { platform: { config }, ... } ──────────────

    fn parse_multiplatform_def(&mut self) -> GravResult<MultiplatformDef> {
        self.advance(); // multiplatform
        self.expect(&TokenKind::LBrace)?;
        let mut platforms = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            self.expect(&TokenKind::LBrace)?;
            let mut config = Vec::new();
            while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                config.push((key, val));
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace)?;
            platforms.push((name, config));
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(MultiplatformDef { platforms })
    }

    // ── Feature N11: migration "name" { body } ──────────────────────────────

    fn parse_migration_def(&mut self) -> GravResult<MigrationDef> {
        self.advance(); // migration
        let name = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("expected migration name string"));
        };
        let body = self.parse_block()?;
        Ok(MigrationDef { name, body })
    }

    // ── Feature N8: debug { body } ──────────────────────────────────────────

    fn parse_debug_stmt(&mut self) -> GravResult<Stmt> {
        self.advance(); // debug
        let body = self.parse_block()?;
        Ok(Stmt::Debug { body })
    }

    // ── Feature N12: simulate/expect_reply in non-scenario context ──────────

    fn parse_simulate_stmt(&mut self) -> GravResult<Stmt> {
        self.advance(); // simulate
        let _user_kw = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let user_id = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        let action = if self.eat(&TokenKind::Sends) {
            SimAction::Sends(self.parse_expr()?)
        } else if self.eat(&TokenKind::Clicks) {
            SimAction::Clicks(self.parse_expr()?)
        } else {
            return Err(self.err("expected 'sends' or 'clicks'"));
        };
        Ok(Stmt::Simulate { user_id, action })
    }

    fn parse_expect_reply_stmt(&mut self) -> GravResult<Stmt> {
        self.advance(); // expect_reply
        let check = if self.eat(&TokenKind::Contains) {
            ExpectCheck::Contains(self.parse_expr()?)
        } else if self.eat(&TokenKind::Equals) {
            ExpectCheck::Equals(self.parse_expr()?)
        } else if self.eat(&TokenKind::Matches_) {
            ExpectCheck::Matches(self.parse_expr()?)
        } else {
            return Err(self.err("expected 'contains', 'equals', or 'matches'"));
        };
        Ok(Stmt::ExpectReply { check })
    }

    // ── Feature W2: table { config } ────────────────────────────────────────

    fn parse_table_stmt(&mut self) -> GravResult<Stmt> {
        self.advance(); // table
        self.expect(&TokenKind::LBrace)?;
        let mut config = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let val = self.parse_expr()?;
            config.push((key, val));
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Stmt::Table { config })
    }

    // ── Feature W3: chart { config } ────────────────────────────────────────

    fn parse_chart_stmt(&mut self) -> GravResult<Stmt> {
        self.advance(); // chart
        self.expect(&TokenKind::LBrace)?;
        let mut config = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let key = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let val = self.parse_expr()?;
            config.push((key, val));
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Stmt::Chart { config })
    }

    // ── Feature W6: stream { body } ─────────────────────────────────────────

    fn parse_stream_stmt(&mut self) -> GravResult<Stmt> {
        self.advance(); // stream
        let body = self.parse_block()?;
        Ok(Stmt::Stream { body })
    }

    // ── Feature W5: webhook "/path" { config, on "event" { body } } ─────────

    fn parse_webhook_def(&mut self) -> GravResult<WebhookDef> {
        self.advance(); // webhook
        let path = if let TokenKind::StrLit(parts) = self.peek().clone() {
            self.advance();
            parts.into_iter()
                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                .collect::<String>()
        } else {
            return Err(self.err("expected webhook path string"));
        };
        self.expect(&TokenKind::LBrace)?;
        let mut config = Vec::new();
        let mut handlers = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            if matches!(self.peek(), TokenKind::On) {
                self.advance(); // on
                let event_name = if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>()
                } else { self.expect_ident()? };
                let body = self.parse_block()?;
                handlers.push((event_name, body));
            } else {
                let key = self.expect_ident()?;
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr()?;
                config.push((key, val));
                self.eat(&TokenKind::Comma);
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(WebhookDef { path, config, handlers })
    }

    // ── Feature W7: permissions { roles: { ... }, default: "role" } ──────────

    fn parse_permissions_def(&mut self) -> GravResult<PermissionsDef> {
        self.advance(); // permissions
        self.expect(&TokenKind::LBrace)?;
        let mut roles: Vec<(String, Vec<String>)> = Vec::new();
        let mut default_role = "user".to_string();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            if self.eat(&TokenKind::Roles) {
                self.expect(&TokenKind::Colon)?;
                self.expect(&TokenKind::LBrace)?;
                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    let role_name = self.expect_ident()?;
                    self.expect(&TokenKind::Colon)?;
                    // Parse permission list: ["perm1", "perm2"]
                    self.expect(&TokenKind::LBracket)?;
                    let mut perms = Vec::new();
                    while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                        if let TokenKind::StrLit(parts) = self.peek().clone() {
                            self.advance();
                            let s = parts.into_iter()
                                .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                                .collect::<String>();
                            perms.push(s);
                        } else {
                            perms.push(self.expect_ident()?);
                        }
                        self.eat(&TokenKind::Comma);
                    }
                    self.expect(&TokenKind::RBracket)?;
                    roles.push((role_name, perms));
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBrace)?;
            } else if self.eat(&TokenKind::Default) {
                self.expect(&TokenKind::Colon)?;
                if let TokenKind::StrLit(parts) = self.peek().clone() {
                    self.advance();
                    default_role = parts.into_iter()
                        .filter_map(|p| if let crate::lexer::StrPart::Lit(s) = p { Some(s) } else { None })
                        .collect::<String>();
                } else {
                    default_role = self.expect_ident()?;
                }
            } else {
                self.advance(); // skip unexpected
            }
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(PermissionsDef { roles, default_role })
    }

    // ── Feature W8: ratelimit { global: N per minute, ... } ─────────────────

    fn parse_ratelimit_def(&mut self) -> GravResult<RatelimitDef> {
        self.advance(); // ratelimit
        self.expect(&TokenKind::LBrace)?;
        let mut rules = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let scope_name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let count_expr = self.parse_expr()?;
            let count = match &count_expr {
                Expr::Int(n) => *n as u32,
                _ => 10,
            };
            self.expect(&TokenKind::Per)?;
            let unit_name = self.expect_ident()?;
            let window_ms: u64 = match unit_name.as_str() {
                "second" | "sec" => 1000,
                "minute" | "min" => 60_000,
                "hour"           => 3_600_000,
                "day"            => 86_400_000,
                _ => 60_000,
            };
            let scope = match scope_name.as_str() {
                "global"  => crate::ast::RatelimitScope::Global,
                "per_user"| "user" => crate::ast::RatelimitScope::PerUser,
                other     => crate::ast::RatelimitScope::Command(other.to_string()),
            };
            rules.push(RatelimitRule { scope, count, window_ms });
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(RatelimitDef { rules })
    }

    // ── Feature W11: typedef Name = base_type [where expr] ──────────────────

    fn parse_typedef_item(&mut self) -> GravResult<TypeDefItem> {
        self.advance(); // typedef
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Eq)?;
        let base_type = self.expect_ident()?;
        let constraint = if self.eat(&TokenKind::Where) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.eat_semi();
        Ok(TypeDefItem { name, base_type, constraint })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    #[allow(unused_imports)]
    use crate::ast::*;

    fn parse(src: &str) -> Program {
        let tokens = Lexer::new(src).tokenize().unwrap();
        Parser::new(tokens).parse().unwrap()
    }

    fn parse_fails(src: &str) -> bool {
        let tokens = Lexer::new(src).tokenize();
        match tokens {
            Err(_) => true,
            Ok(toks) => Parser::new(toks).parse().is_err(),
        }
    }

    fn first_item(src: &str) -> Item {
        parse(src).items.into_iter().next().unwrap()
    }

    fn first_stmt(src: &str) -> Stmt {
        match first_item(&format!("fn _t() {{ {} }}", src)) {
            Item::FnDef(fd) => fd.body.into_iter().next().unwrap(),
            other => panic!("expected FnDef, got {:?}", other),
        }
    }

    // ── Function definitions ─────────────────────────────────────────────────

    #[test]
    fn parse_empty_fn() {
        match first_item("fn greet() { }") {
            Item::FnDef(fd) => {
                assert_eq!(fd.name, "greet");
                assert!(fd.params.is_empty());
                assert!(fd.body.is_empty());
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn parse_fn_with_params_and_return() {
        match first_item("fn add(a: int, b: int) -> int { return a + b }") {
            Item::FnDef(fd) => {
                assert_eq!(fd.name, "add");
                assert_eq!(fd.params.len(), 2);
                assert_eq!(fd.params[0].name, "a");
                assert!(fd.ret.is_some());
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn parse_fn_with_decorator() {
        match first_item("@retry(3)\nfn fetch() { }") {
            Item::FnDef(fd) => {
                assert_eq!(fd.name, "fetch");
                assert_eq!(fd.decorators.len(), 1);
                assert_eq!(fd.decorators[0].name, "retry");
            }
            _ => panic!("expected FnDef"),
        }
    }

    #[test]
    fn parse_fn_default_param() {
        match first_item("fn greet(name: str = \"world\") { }") {
            Item::FnDef(fd) => {
                assert_eq!(fd.params[0].name, "name");
                assert!(fd.params[0].default.is_some());
            }
            _ => panic!("expected FnDef"),
        }
    }

    // ── Handlers ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_on_command() {
        match first_item("on /start { emit \"hello\" }") {
            Item::Handler(h) => {
                assert!(matches!(h.trigger, Trigger::Command(ref c) if c == "start"));
                assert_eq!(h.body.len(), 1);
            }
            _ => panic!("expected Handler"),
        }
    }

    #[test]
    fn parse_on_msg() {
        match first_item("on msg { emit \"echo\" }") {
            Item::Handler(h) => {
                assert!(matches!(h.trigger, Trigger::AnyMsg));
            }
            _ => panic!("expected Handler"),
        }
    }

    #[test]
    fn parse_on_msg_with_guard() {
        match first_item("on msg guard ctx.text != \"\" { emit \"echo\" }") {
            Item::Handler(h) => {
                assert!(h.guard.is_some());
            }
            _ => panic!("expected Handler"),
        }
    }

    #[test]
    fn parse_on_callback() {
        match first_item("on callback \"yes\" { }") {
            Item::Handler(h) => {
                assert!(matches!(h.trigger, Trigger::Callback(Some(ref s)) if s == "yes"));
            }
            _ => panic!("expected Handler"),
        }
    }

    #[test]
    fn parse_on_join() {
        match first_item("on join { }") {
            Item::Handler(h) => {
                assert!(matches!(h.trigger, Trigger::Join));
            }
            _ => panic!("expected Handler"),
        }
    }

    #[test]
    fn parse_on_reaction() {
        match first_item("on reaction { }") {
            Item::Handler(h) => {
                assert!(matches!(h.trigger, Trigger::Reaction(None)));
            }
            _ => panic!("expected Handler"),
        }
    }

    // ── Statements ───────────────────────────────────────────────────────────

    #[test]
    fn parse_let_stmt() {
        match first_stmt("let x = 42") {
            Stmt::Let { name, ty, .. } => {
                assert_eq!(name, "x");
                assert!(ty.is_none());
            }
            other => panic!("expected Let, got {:?}", other),
        }
    }

    #[test]
    fn parse_let_typed() {
        match first_stmt("let x: int = 42") {
            Stmt::Let { name, ty, .. } => {
                assert_eq!(name, "x");
                assert!(ty.is_some());
            }
            other => panic!("expected Let, got {:?}", other),
        }
    }

    #[test]
    fn parse_assign() {
        match first_stmt("x = 10") {
            Stmt::Assign { .. } => {}
            other => panic!("expected Assign, got {:?}", other),
        }
    }

    #[test]
    fn parse_compound_assign() {
        match first_stmt("x += 5") {
            Stmt::CompoundAssign { op, .. } => {
                assert_eq!(op, BinOp::Add);
            }
            other => panic!("expected CompoundAssign, got {:?}", other),
        }
    }

    #[test]
    fn parse_emit() {
        match first_stmt("emit \"hello\"") {
            Stmt::Emit(_) => {}
            other => panic!("expected Emit, got {:?}", other),
        }
    }

    #[test]
    fn parse_emit_broadcast() {
        match first_stmt("emit broadcast \"hi\"") {
            Stmt::EmitBroadcast(_) => {}
            other => panic!("expected EmitBroadcast, got {:?}", other),
        }
    }

    #[test]
    fn parse_return() {
        match first_stmt("return 42") {
            Stmt::Return(Some(_)) => {}
            other => panic!("expected Return, got {:?}", other),
        }
    }

    #[test]
    fn parse_return_void() {
        match first_stmt("return") {
            Stmt::Return(None) => {}
            other => panic!("expected Return(None), got {:?}", other),
        }
    }

    #[test]
    fn parse_if_else() {
        match first_stmt("if x > 0 { emit \"pos\" } else { emit \"neg\" }") {
            Stmt::If { then, else_, .. } => {
                assert!(!then.is_empty());
                assert!(else_.is_some());
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn parse_while_loop() {
        match first_stmt("while x > 0 { x -= 1 }") {
            Stmt::While { .. } => {}
            other => panic!("expected While, got {:?}", other),
        }
    }

    #[test]
    fn parse_for_loop() {
        match first_stmt("for i in [1, 2, 3] { emit i }") {
            Stmt::For { var, .. } => {
                assert_eq!(var, "i");
            }
            other => panic!("expected For, got {:?}", other),
        }
    }

    #[test]
    fn parse_match_stmt() {
        match first_stmt("match x { 1 => { emit \"one\" } }") {
            Stmt::Match { arms, .. } => {
                assert!(!arms.is_empty());
            }
            other => panic!("expected Match, got {:?}", other),
        }
    }

    #[test]
    fn parse_try_catch() {
        match first_stmt("try { emit \"ok\" } catch e { emit \"err\" }") {
            Stmt::TryCatch { err_name, .. } => {
                assert_eq!(err_name, "e");
            }
            other => panic!("expected TryCatch, got {:?}", other),
        }
    }

    #[test]
    fn parse_try_catch_finally() {
        match first_stmt("try { emit \"ok\" } catch e { emit \"err\" } finally { emit \"done\" }") {
            Stmt::TryCatch { finally_body, .. } => {
                assert!(!finally_body.is_empty());
            }
            other => panic!("expected TryCatch, got {:?}", other),
        }
    }

    #[test]
    fn parse_break_continue() {
        assert!(matches!(first_stmt("break"), Stmt::Break));
        assert!(matches!(first_stmt("continue"), Stmt::Continue));
    }

    #[test]
    fn parse_fire_event() {
        match first_stmt("fire \"custom_event\" 42") {
            Stmt::Fire { .. } => {}
            other => panic!("expected Fire, got {:?}", other),
        }
    }

    #[test]
    fn parse_assert_stmt() {
        match first_stmt("assert 2 + 2 == 4") {
            Stmt::Assert { .. } => {}
            other => panic!("expected Assert, got {:?}", other),
        }
    }

    // ── Top-level constructs ─────────────────────────────────────────────────

    #[test]
    fn parse_import() {
        match first_item("import \"utils.grav\"") {
            Item::Import(path) => assert_eq!(path, "utils.grav"),
            other => panic!("expected Import, got {:?}", other),
        }
    }

    #[test]
    fn parse_state_def() {
        match first_item("state { count: int = 0 }") {
            Item::StateDef(sd) => {
                assert_eq!(sd.fields.len(), 1);
                assert_eq!(sd.fields[0].name, "count");
            }
            other => panic!("expected StateDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_struct_def() {
        match first_item("struct Point { x: int, y: int }") {
            Item::StructDef(sd) => {
                assert_eq!(sd.name, "Point");
                assert_eq!(sd.fields.len(), 2);
            }
            other => panic!("expected StructDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_enum_def() {
        match first_item("enum Color { Red, Green, Blue }") {
            Item::EnumDef(ed) => {
                assert_eq!(ed.name, "Color");
                assert_eq!(ed.variants.len(), 3);
                assert_eq!(ed.variants[0].name, "Red");
            }
            other => panic!("expected EnumDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_flow_def() {
        match first_item("flow my_flow { emit \"step 1\" }") {
            Item::FlowDef(fd) => {
                assert_eq!(fd.name, "my_flow");
                assert!(!fd.body.is_empty());
            }
            other => panic!("expected FlowDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_test_def() {
        match first_item("test \"math\" { assert 2 + 2 == 4 }") {
            Item::TestDef(td) => {
                assert_eq!(td.name, "math");
                assert!(!td.body.is_empty());
            }
            other => panic!("expected TestDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_schedule_def() {
        match first_item("schedule \"0 9 * * *\" { emit \"morning\" }") {
            Item::ScheduleDef(sd) => {
                assert_eq!(sd.cron, "0 9 * * *");
            }
            other => panic!("expected ScheduleDef, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_path() {
        match first_item("use \"utils.grav\"") {
            Item::Use(path) => assert_eq!(path, "utils.grav"),
            other => panic!("expected Use, got {:?}", other),
        }
    }

    // ── Expressions ──────────────────────────────────────────────────────────

    #[test]
    fn parse_int_expr() {
        let s = first_stmt("42");
        assert!(matches!(s, Stmt::Expr(Expr::Int(42))));
    }

    #[test]
    fn parse_float_expr() {
        let s = first_stmt("3.14");
        if let Stmt::Expr(Expr::Float(f)) = s {
            assert!((f - 3.14).abs() < 1e-10);
        } else {
            panic!("expected Float expr");
        }
    }

    #[test]
    fn parse_string_expr() {
        let s = first_stmt("\"hello\"");
        assert!(matches!(s, Stmt::Expr(Expr::Str(_))));
    }

    #[test]
    fn parse_list_literal() {
        let s = first_stmt("[1, 2, 3]");
        if let Stmt::Expr(Expr::List(items)) = s {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn parse_binary_precedence() {
        // x + y * 2 should parse as x + (y * 2)
        let s = first_stmt("x + y * 2");
        if let Stmt::Expr(Expr::Binary { op, rhs, .. }) = s {
            assert_eq!(op, BinOp::Add);
            assert!(matches!(*rhs, Expr::Binary { op: BinOp::Mul, .. }));
        } else {
            panic!("expected Binary");
        }
    }

    #[test]
    fn parse_pipe_expr() {
        let s = first_stmt("items |> sort");
        assert!(matches!(s, Stmt::Expr(Expr::Pipe { .. })));
    }

    #[test]
    fn parse_null_coalesce() {
        let s = first_stmt("x ?? \"default\"");
        if let Stmt::Expr(Expr::Binary { op, .. }) = s {
            assert_eq!(op, BinOp::NullCoalesce);
        } else {
            panic!("expected Binary NullCoalesce");
        }
    }

    #[test]
    fn parse_optional_chaining() {
        let s = first_stmt("obj?.name");
        // Should produce a Field access with some optional chaining representation
        assert!(matches!(s, Stmt::Expr(_)));
    }

    #[test]
    fn parse_lambda() {
        let s = first_stmt("fn(x) { x + 1 }");
        if let Stmt::Expr(Expr::Lambda { params, body }) = s {
            assert_eq!(params.len(), 1);
            assert!(!body.is_empty());
        } else {
            panic!("expected Lambda");
        }
    }

    #[test]
    fn parse_fn_call() {
        let s = first_stmt("greet(\"world\")");
        assert!(matches!(s, Stmt::Expr(Expr::Call { .. })));
    }

    #[test]
    fn parse_method_call() {
        let s = first_stmt("items.push(42)");
        assert!(matches!(s, Stmt::Expr(Expr::Method { .. })));
    }

    #[test]
    fn parse_field_access() {
        let s = first_stmt("ctx.text");
        assert!(matches!(s, Stmt::Expr(Expr::Field { .. })));
    }

    #[test]
    fn parse_index_access() {
        let s = first_stmt("items[0]");
        assert!(matches!(s, Stmt::Expr(Expr::Index { .. })));
    }

    // ── Multiple items ───────────────────────────────────────────────────────

    #[test]
    fn parse_multiple_items() {
        let prog = parse("fn a() { }\nfn b() { }");
        assert_eq!(prog.items.len(), 2);
    }

    #[test]
    fn parse_handler_and_fn() {
        let prog = parse("on /start { emit \"hi\" }\nfn helper() { }");
        assert_eq!(prog.items.len(), 2);
        assert!(matches!(prog.items[0], Item::Handler(_)));
        assert!(matches!(prog.items[1], Item::FnDef(_)));
    }

    // ── Error cases ──────────────────────────────────────────────────────────

    #[test]
    fn parse_fn_missing_name() {
        assert!(parse_fails("fn { }"));
    }

    #[test]
    fn parse_let_missing_name() {
        assert!(parse_fails("fn t() { let = 42 }"));
    }

    #[test]
    fn parse_empty_program() {
        let prog = parse("");
        assert!(prog.items.is_empty());
    }
}
