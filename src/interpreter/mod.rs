pub mod env;
pub mod eval;
pub mod exec;
pub mod dispatch;

use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Mutex;
use regex::Regex;

use crate::ast::{FnDef, FlowDef, EveryDef, AtDef, Expr, FsmDef, ScheduleDef, HookDef, AbTestDef, MetricDef, Stmt, EnumDef, EntityDef, CanaryDef};
use crate::value::{Value, BotOutput};
use crate::runtime_err;

pub use self::env::Env;

pub struct SharedState {
    pub bot_state:          HashMap<String, Value>,
    pub regex_cache:        HashMap<String, Regex>,
    pub functions:          HashMap<String, Rc<FnDef>>,
    pub flows:              HashMap<String, FlowDef>,
    pub every_defs:         Vec<EveryDef>,
    pub at_defs:            Vec<AtDef>,
    /// Pending wait_msg channels: (room_id, user_id) -> Sender
    pub wait_map:           HashMap<(i64, i64), tokio::sync::oneshot::Sender<String>>,
    /// Pending wait_callback channels: (room_id, user_id) -> Sender
    pub callback_wait_map:  HashMap<(i64, i64), tokio::sync::oneshot::Sender<String>>,
    /// All room IDs the bot has ever seen (for broadcast)
    pub known_rooms:        Vec<i64>,
    /// Vortex bot API token
    pub bot_token:          String,
    /// Vortex server base URL
    pub vortex_url:         String,
    /// Call stack for traceback
    pub call_stack:         Vec<String>,
    /// Persistent key-value database
    pub db:                 crate::stdlib::db::Db,
    /// Rate limit tracking: key = (room_id, user_id, handler_index) -> list of timestamps ms
    pub rate_limits:        HashMap<(i64, i64, usize), std::collections::VecDeque<u64>>,
    /// Per-user state: (user_id, field_name) -> Value
    pub per_user_state:     HashMap<(i64, String), Value>,
    /// Per-room state: (room_id, field_name) -> Value
    pub per_room_state:     HashMap<(i64, String), Value>,
    /// State field definitions (for scope lookup)
    pub state_defs:         HashMap<String, crate::ast::StateField>,
    /// FSM definitions: name -> FsmDef
    pub fsm_defs:           HashMap<String, FsmDef>,
    /// Per-user FSM state: (user_id, fsm_name) -> current_state_name
    pub fsm_states:         HashMap<(i64, String), String>,
    /// Permission definitions: name -> condition Expr
    pub permissions:        HashMap<String, Expr>,
    /// TTL cache: key -> (value, expires_at_unix_ms)
    pub cache_store:        HashMap<String, (Value, u64)>,
    /// Cron schedule definitions
    pub schedule_defs:      Vec<ScheduleDef>,
    /// Before-hooks (Feature 3)
    pub before_hooks:       Vec<Vec<Stmt>>,
    /// After-hooks (Feature 3)
    pub after_hooks:        Vec<Vec<Stmt>>,
    /// Loaded plugins (Feature 5)
    pub loaded_plugins:     HashSet<String>,
    /// Bot metrics counters/gauges (Feature 9)
    pub bot_metrics:        HashMap<String, f64>,
    /// Bot metrics histograms (Feature 9)
    pub bot_histograms:     HashMap<String, Vec<f64>>,
    /// Registered metric names (Feature 9)
    pub metric_names:       Vec<MetricDef>,
    /// Message history per (room_id, user_id) (Feature 10)
    pub message_history:    HashMap<(i64, i64), VecDeque<Value>>,
    /// A/B test results (name -> (a_count, b_count)) (Feature 11)
    pub ab_results:         HashMap<String, (u64, u64)>,
    /// A/B test definitions (Feature 11)
    pub ab_tests:           HashMap<String, AbTestDef>,
    /// Last activity per user (user_id -> timestamp_ms) (Feature 8)
    pub last_activity:      HashMap<i64, u64>,
    /// Last known room per user (user_id -> room_id) (Feature 8)
    pub last_room:          HashMap<i64, i64>,
    /// Before-hook definitions with bodies (Feature 3)
    pub before_hook_defs:   Vec<HookDef>,
    /// After-hook definitions with bodies (Feature 3)
    pub after_hook_defs:    Vec<HookDef>,
    /// i18n strings: locale -> key -> value (Feature 12)
    pub i18n_strings:       HashMap<String, HashMap<String, Value>>,
    /// Default language for i18n (Feature 12)
    #[allow(dead_code)]
    pub default_lang:       String,
    /// Webhook registered paths (Feature 10)
    #[allow(dead_code)]
    pub webhook_paths:      HashSet<String>,
    /// Pagination state: (room_id, user_id) -> PaginationState (Feature 11)
    #[allow(dead_code)]
    pub paginations:        HashMap<(i64, i64), PaginationState>,
    /// Enum definitions: name -> EnumDef
    pub enum_defs:          HashMap<String, EnumDef>,
    /// Impl methods: (type_name, method_name) -> FnDef
    pub impl_methods:       HashMap<(String, String), Rc<FnDef>>,
    /// Queue state: name -> QueueState
    #[allow(dead_code)]
    pub queues:             HashMap<String, QueueState>,
    /// Denied function names for sandbox mode (Feature 12)
    pub denied_fns:         HashSet<String>,
    /// State field watchers: field_name -> list of watcher bodies (Feature 3)
    pub watchers:           HashMap<String, Vec<Vec<Stmt>>>,
    /// Event handlers: event_name -> list of handler bodies (Feature 2)
    pub event_handlers:     HashMap<String, Vec<Vec<Stmt>>>,
    /// Active mocks: fn_name -> mock body (Feature 5)
    pub mocks:              HashMap<String, Vec<Stmt>>,
    /// Middleware definitions (Feature 11)
    pub middleware_defs:    HashMap<String, crate::ast::MiddlewareDef>,
    /// Active middleware chain (Feature 11)
    pub middleware_chain:   Vec<String>,
    /// Admin definition (Feature 9)
    #[allow(dead_code)]
    pub admin_def:          Option<crate::ast::AdminDef>,

    // ── New features N1-N12 ──────────────────────────────────────────────────

    /// Intent definitions: intent_name -> phrases (Feature N1)
    pub intent_defs:        HashMap<String, Vec<String>>,
    /// Entity definitions (Feature N2)
    pub entity_defs:        Vec<EntityDef>,
    /// Circuit breaker states (Feature N3)
    pub breakers:           HashMap<String, BreakerState>,
    /// Analytics events (Feature N4)
    pub analytics:          Vec<AnalyticsEvent>,
    /// Canary definitions (Feature N5)
    pub canaries:           Vec<CanaryDef>,
    /// Channels for inter-spawn communication (Feature N6)
    pub channels:           HashMap<String, VecDeque<Value>>,
    /// Platform configurations (Feature N10)
    pub platforms:          HashMap<String, HashMap<String, Value>>,
    /// Completed migrations (Feature N11)
    pub completed_migrations: HashSet<String>,

    // ── New features W1-W12 ─────────────────────────────────────────────────

    /// Webhook definitions: path -> WebhookDef (Feature W5)
    #[allow(dead_code)]
    pub webhook_defs:         Vec<crate::ast::WebhookDef>,
    /// RBAC permissions definition (Feature W7)
    #[allow(dead_code)]
    pub rbac_roles:           HashMap<String, Vec<String>>,
    /// RBAC default role (Feature W7)
    #[allow(dead_code)]
    pub rbac_default_role:    String,
    /// User roles: user_id -> role_name (Feature W7)
    pub user_roles:           HashMap<i64, String>,
    /// Ratelimit definitions (Feature W8)
    #[allow(dead_code)]
    pub ratelimit_rules:      Vec<crate::ast::RatelimitRule>,
    /// Type definitions: name -> TypeDefItem (Feature W11)
    #[allow(dead_code)]
    pub type_defs:            HashMap<String, crate::ast::TypeDefItem>,
    /// Imported file paths (Feature W9)
    pub imported_files:       HashSet<String>,
    /// WebSocket configs (Feature W4)
    #[allow(dead_code)]
    pub ws_configs:           Vec<(String, HashMap<String, Value>)>,
}

/// Circuit breaker state (Feature N3)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BreakerState {
    pub threshold:    u32,
    pub timeout_ms:   u64,
    pub failure_count: u32,
    pub status:       BreakerStatus,
    pub last_failure: u64,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum BreakerStatus {
    Closed,
    Open,
    HalfOpen,
}

/// Analytics event (Feature N4)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AnalyticsEvent {
    pub name:      String,
    pub data:      Value,
    pub timestamp: u64,
}

/// Queue state stored in SharedState
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct QueueState {
    pub concurrency: usize,
    pub retry:       usize,
    pub pending:     VecDeque<Vec<Stmt>>,
    pub running:     usize,
}

/// Pagination state stored in SharedState (Feature 11)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PaginationState {
    pub items:     Vec<Value>,
    pub page:      usize,
    pub page_size: usize,
    pub title:     String,
}

impl SharedState {
    pub fn new(token: String, vortex_url: String) -> Self {
        Self {
            bot_state:         HashMap::new(),
            regex_cache:       HashMap::new(),
            functions:         HashMap::new(),
            flows:             HashMap::new(),
            every_defs:        Vec::new(),
            at_defs:           Vec::new(),
            wait_map:          HashMap::new(),
            callback_wait_map: HashMap::new(),
            known_rooms:       Vec::new(),
            bot_token:         token,
            vortex_url,
            call_stack:        Vec::new(),
            db:                crate::stdlib::db::Db::new(Some(std::path::PathBuf::from("bot_data.json"))),
            rate_limits:       HashMap::new(),
            per_user_state:    HashMap::new(),
            per_room_state:    HashMap::new(),
            state_defs:        HashMap::new(),
            fsm_defs:          HashMap::new(),
            fsm_states:        HashMap::new(),
            permissions:       HashMap::new(),
            cache_store:       HashMap::new(),
            schedule_defs:     Vec::new(),
            before_hooks:      Vec::new(),
            after_hooks:       Vec::new(),
            loaded_plugins:    HashSet::new(),
            bot_metrics:       HashMap::new(),
            bot_histograms:    HashMap::new(),
            metric_names:      Vec::new(),
            message_history:   HashMap::new(),
            ab_results:        HashMap::new(),
            ab_tests:          HashMap::new(),
            last_activity:     HashMap::new(),
            last_room:         HashMap::new(),
            before_hook_defs:  Vec::new(),
            after_hook_defs:   Vec::new(),
            i18n_strings:      HashMap::new(),
            default_lang:      "en".to_string(),
            webhook_paths:     HashSet::new(),
            paginations:       HashMap::new(),
            enum_defs:         HashMap::new(),
            impl_methods:      HashMap::new(),
            queues:            HashMap::new(),
            denied_fns:        HashSet::new(),
            watchers:          HashMap::new(),
            event_handlers:    HashMap::new(),
            mocks:             HashMap::new(),
            middleware_defs:   HashMap::new(),
            middleware_chain:  Vec::new(),
            admin_def:         None,
            intent_defs:       HashMap::new(),
            entity_defs:       Vec::new(),
            breakers:          HashMap::new(),
            analytics:         Vec::new(),
            canaries:          Vec::new(),
            channels:          HashMap::new(),
            platforms:         HashMap::new(),
            completed_migrations: HashSet::new(),
            webhook_defs:        Vec::new(),
            rbac_roles:          HashMap::new(),
            rbac_default_role:   "user".to_string(),
            user_roles:          HashMap::new(),
            ratelimit_rules:     Vec::new(),
            type_defs:           HashMap::new(),
            imported_files:      HashSet::new(),
            ws_configs:          Vec::new(),
        }
    }

    pub fn check_ratelimit(&mut self, rl: &crate::ast::RateLimit, room_id: i64, user_id: i64, handler_idx: usize) -> bool {
        let key = match rl.per {
            crate::ast::RateLimitScope::User   => (room_id, user_id, handler_idx),
            crate::ast::RateLimitScope::Room   => (room_id, 0, handler_idx),
            crate::ast::RateLimitScope::Global => (0, 0, handler_idx),
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let entry = self.rate_limits.entry(key).or_default();
        // Remove expired timestamps
        while entry.front().map_or(false, |&t| now - t > rl.window_ms) {
            entry.pop_front();
        }
        if entry.len() < rl.count as usize {
            entry.push_back(now);
            true // allowed
        } else {
            false // rate limited
        }
    }

    pub fn get_state_field(&self, field: &str, ctx_user_id: i64, ctx_room_id: i64) -> Value {
        if let Some(scope) = self.state_field_scope(field) {
            match scope {
                crate::ast::StateScope::PerUser => self.per_user_state.get(&(ctx_user_id, field.to_string())).cloned().unwrap_or(Value::Null),
                crate::ast::StateScope::PerRoom => self.per_room_state.get(&(ctx_room_id, field.to_string())).cloned().unwrap_or(Value::Null),
                crate::ast::StateScope::Global  => self.bot_state.get(field).cloned().unwrap_or(Value::Null),
            }
        } else {
            self.bot_state.get(field).cloned().unwrap_or(Value::Null)
        }
    }

    pub fn set_state_field(&mut self, field: &str, val: Value, ctx_user_id: i64, ctx_room_id: i64) {
        match self.state_field_scope(field) {
            Some(crate::ast::StateScope::PerUser) => { self.per_user_state.insert((ctx_user_id, field.to_string()), val); }
            Some(crate::ast::StateScope::PerRoom) => { self.per_room_state.insert((ctx_room_id, field.to_string()), val); }
            _ => { self.bot_state.insert(field.to_string(), val); }
        }
    }

    fn state_field_scope(&self, field: &str) -> Option<crate::ast::StateScope> {
        self.state_defs.get(field).map(|f| f.scope.clone())
    }

    /// Prometheus-format metrics text (Feature 9)
    #[allow(dead_code)]
    pub fn get_metrics_text(&self) -> String {
        let mut out = String::new();
        for metric in &self.metric_names {
            let kind_str = match metric.kind {
                crate::ast::MetricKind::Counter   => "counter",
                crate::ast::MetricKind::Gauge     => "gauge",
                crate::ast::MetricKind::Histogram => "histogram",
            };
            out.push_str(&format!("# TYPE {} {}\n", metric.name, kind_str));
            if let Some(v) = self.bot_metrics.get(&metric.name) {
                out.push_str(&format!("{} {}\n", metric.name, v));
            }
            if let Some(samples) = self.bot_histograms.get(&metric.name) {
                let sum: f64 = samples.iter().sum();
                let count = samples.len();
                out.push_str(&format!("{}_sum {}\n", metric.name, sum));
                out.push_str(&format!("{}_count {}\n", metric.name, count));
            }
        }
        out
    }

    pub fn get_or_compile_regex(&mut self, pattern: &str, flags: &str) -> crate::error::GravResult<Regex> {
        let key = format!("{pattern}/{flags}");
        if let Some(r) = self.regex_cache.get(&key) { return Ok(r.clone()); }
        let prefix = if flags.contains('i') { "(?i)" } else { "" };
        let re = Regex::new(&format!("{prefix}{pattern}"))
            .map_err(|e| runtime_err!("invalid regex /{pattern}/{flags}: {e}"))?;
        self.regex_cache.insert(key, re.clone());
        Ok(re)
    }
}

pub struct Interpreter {
    pub shared: Arc<Mutex<SharedState>>,
}

impl Interpreter {
    pub fn new(token: String, vortex_url: String) -> Self {
        Self { shared: Arc::new(Mutex::new(SharedState::new(token, vortex_url))) }
    }

    pub async fn load(&self, prog: &crate::ast::Program) -> crate::error::GravResult<()> {
        let mut st = self.shared.lock().await;
        for item in &prog.items {
            match item {
                crate::ast::Item::FnDef(fd) => {
                    st.functions.insert(fd.name.clone(), Rc::new(fd.clone()));
                }
                crate::ast::Item::FlowDef(fd) => {
                    st.flows.insert(fd.name.clone(), fd.clone());
                }
                crate::ast::Item::StateDef(sd) => {
                    // Collect field info before borrowing st mutably
                    let fields_snapshot: Vec<crate::ast::StateField> = sd.fields.clone();
                    for field in &fields_snapshot {
                        // Store field definition for scope lookup
                        st.state_defs.insert(field.name.clone(), field.clone());
                        if let Some(default_expr) = &field.default {
                            let field_name = field.name.clone();
                            let field_scope = field.scope.clone();
                            let expr = default_expr.clone();
                            let mut env = Env::new();
                            drop(st);
                            let val = self.eval_expr(&expr, &mut env, None).await?;
                            st = self.shared.lock().await;
                            // Only store default in global bot_state for Global scope
                            if field_scope == crate::ast::StateScope::Global {
                                st.bot_state.insert(field_name, val);
                            }
                        } else if field.scope == crate::ast::StateScope::Global {
                            st.bot_state.insert(field.name.clone(), Value::Null);
                        }
                    }
                }
                crate::ast::Item::Every(e) => { st.every_defs.push(e.clone()); }
                crate::ast::Item::At(a)    => { st.at_defs.push(a.clone()); }
                crate::ast::Item::FsmDef(f) => { st.fsm_defs.insert(f.name.clone(), f.clone()); }
                crate::ast::Item::PermDef(p) => { st.permissions.insert(p.name.clone(), p.cond.clone()); }
                crate::ast::Item::ScheduleDef(s) => { st.schedule_defs.push(s.clone()); }
                crate::ast::Item::HookDef(h) => {
                    match h.when {
                        crate::ast::HookWhen::Before => {
                            st.before_hooks.push(h.body.clone());
                            st.before_hook_defs.push(h.clone());
                        }
                        crate::ast::HookWhen::After => {
                            st.after_hooks.push(h.body.clone());
                            st.after_hook_defs.push(h.clone());
                        }
                    }
                }
                crate::ast::Item::PluginDef(p) => {
                    st.loaded_plugins.insert(p.name.clone());
                    // Plugin loading: log a notice (actual file loading would happen here)
                    eprintln!("[gravitix] plugin '{}' registered (config: {} keys)", p.name, p.config.len());
                }
                crate::ast::Item::MetricsDef(m) => {
                    for def in &m.defs {
                        st.metric_names.push(def.clone());
                        if def.kind != crate::ast::MetricKind::Histogram {
                            st.bot_metrics.entry(def.name.clone()).or_insert(0.0);
                        } else {
                            st.bot_histograms.entry(def.name.clone()).or_default();
                        }
                    }
                }
                crate::ast::Item::AbTestItem(ab) => {
                    st.ab_tests.insert(ab.name.clone(), ab.clone());
                }
                crate::ast::Item::LangDef(ld) => {
                    for (locale, pairs) in &ld.locales {
                        let mut kv = HashMap::new();
                        for (key, val_expr) in pairs {
                            let val_key = key.clone();
                            let expr = val_expr.clone();
                            let locale_code = locale.clone();
                            let mut env = Env::new();
                            drop(st);
                            let val = self.eval_expr(&expr, &mut env, None).await?;
                            st = self.shared.lock().await;
                            let _ = locale_code;
                            kv.insert(val_key, val);
                        }
                        st.i18n_strings.insert(locale.clone(), kv);
                    }
                }
                crate::ast::Item::EnumDef(ed) => {
                    st.enum_defs.insert(ed.name.clone(), ed.clone());
                }
                crate::ast::Item::ImplBlock(ib) => {
                    for method in &ib.methods {
                        st.impl_methods.insert(
                            (ib.type_name.clone(), method.name.clone()),
                            Rc::new(method.clone()),
                        );
                    }
                }
                crate::ast::Item::QueueDef(qd) => {
                    st.queues.insert(qd.name.clone(), QueueState {
                        concurrency: 1,
                        retry:       0,
                        pending:     VecDeque::new(),
                        running:     0,
                    });
                }
                crate::ast::Item::WatchDef(wd) => {
                    st.watchers.entry(wd.field.clone()).or_default().push(wd.body.clone());
                }
                crate::ast::Item::AdminDef(ad) => {
                    st.admin_def = Some(ad.clone());
                }
                crate::ast::Item::MiddlewareDef(md) => {
                    st.middleware_defs.insert(md.name.clone(), md.clone());
                }
                crate::ast::Item::Handler(h) => {
                    // Collect event handlers into the event_handlers map
                    if let crate::ast::Trigger::Event(ref name) = h.trigger {
                        st.event_handlers.entry(name.clone()).or_default().push(h.body.clone());
                    }
                }
                crate::ast::Item::IntentsDef(id) => {
                    for (name, phrases) in &id.intents {
                        st.intent_defs.insert(name.clone(), phrases.clone());
                    }
                }
                crate::ast::Item::EntitiesDef(ed) => {
                    st.entity_defs = ed.entities.clone();
                }
                crate::ast::Item::CircuitBreakerDef(cbd) => {
                    let mut threshold: u32 = 5;
                    let mut timeout_ms: u64 = 30000;
                    for (key, val_expr) in &cbd.config {
                        let expr = val_expr.clone();
                        let k = key.clone();
                        let name = cbd.name.clone();
                        let mut env = Env::new();
                        drop(st);
                        let val = self.eval_expr(&expr, &mut env, None).await?;
                        st = self.shared.lock().await;
                        let _ = name;
                        match k.as_str() {
                            "threshold" => threshold = val.as_int().unwrap_or(5) as u32,
                            "timeout"   => timeout_ms = val.as_int().unwrap_or(30000) as u64,
                            _ => {}
                        }
                    }
                    st.breakers.insert(cbd.name.clone(), BreakerState {
                        threshold,
                        timeout_ms,
                        failure_count: 0,
                        status: BreakerStatus::Closed,
                        last_failure: 0,
                    });
                }
                crate::ast::Item::CanaryDef(cd) => {
                    st.canaries.push(cd.clone());
                }
                crate::ast::Item::MultiplatformDef(mpd) => {
                    for (platform, config_pairs) in &mpd.platforms {
                        let mut config_map = HashMap::new();
                        for (key, val_expr) in config_pairs {
                            let expr = val_expr.clone();
                            let k = key.clone();
                            let mut env = Env::new();
                            drop(st);
                            let val = self.eval_expr(&expr, &mut env, None).await?;
                            st = self.shared.lock().await;
                            config_map.insert(k, val);
                        }
                        st.platforms.insert(platform.clone(), config_map);
                    }
                }
                crate::ast::Item::MigrationDef(md) => {
                    let name = md.name.clone();
                    if !st.completed_migrations.contains(&name) {
                        let body = md.body.clone();
                        st.completed_migrations.insert(name.clone());
                        // Save to DB
                        let migrations_key = "_migrations".to_string();
                        let val = Value::make_str(&name);
                        st.db.set("_migrations", &name, val);
                        drop(st);
                        let mut env = Env::new();
                        let mut outputs = Vec::new();
                        let _ = self.exec_block(&body, &mut env, None, &mut outputs).await;
                        st = self.shared.lock().await;
                        let _ = migrations_key;
                        eprintln!("[gravitix] migration '{}' executed", name);
                    }
                }
                crate::ast::Item::UsePkg(name) => {
                    // Try to load from plugins/{name}.grav
                    let path = format!("plugins/{name}.grav");
                    if let Ok(src) = std::fs::read_to_string(&path) {
                        match crate::lexer::Lexer::new(&src).tokenize()
                            .and_then(|tokens| crate::parser::Parser::new(tokens).parse())
                        {
                            Ok(pkg_prog) => {
                                drop(st);
                                Box::pin(self.load(&pkg_prog)).await?;
                                st = self.shared.lock().await;
                                eprintln!("[gravitix] package '{name}' loaded from {path}");
                            }
                            Err(e) => {
                                eprintln!("[gravitix] package '{name}' parse error: {e}");
                            }
                        }
                    } else {
                        eprintln!("[gravitix] package '{name}' not found at {path}");
                    }
                }
                crate::ast::Item::WebhookDef(wd) => {
                    st.webhook_paths.insert(wd.path.clone());
                    st.webhook_defs.push(wd.clone());
                }
                crate::ast::Item::PermissionsDef(pd) => {
                    for (role, perms) in &pd.roles {
                        st.rbac_roles.insert(role.clone(), perms.clone());
                    }
                    st.rbac_default_role = pd.default_role.clone();
                }
                crate::ast::Item::RatelimitDef(rd) => {
                    st.ratelimit_rules = rd.rules.clone();
                }
                crate::ast::Item::Import(path) => {
                    if !st.imported_files.contains(path) {
                        st.imported_files.insert(path.clone());
                        let import_path = path.clone();
                        drop(st);
                        if let Ok(src) = std::fs::read_to_string(&import_path) {
                            match crate::lexer::Lexer::new(&src).tokenize()
                                .and_then(|tokens| crate::parser::Parser::new(tokens).parse())
                            {
                                Ok(import_prog) => {
                                    Box::pin(self.load(&import_prog)).await?;
                                    eprintln!("[gravitix] imported '{import_path}'");
                                }
                                Err(e) => {
                                    eprintln!("[gravitix] import '{import_path}' parse error: {e}");
                                }
                            }
                        } else {
                            eprintln!("[gravitix] import '{import_path}' not found");
                        }
                        st = self.shared.lock().await;
                    }
                }
                crate::ast::Item::TypeDefItem(td) => {
                    st.type_defs.insert(td.name.clone(), td.clone());
                }
                crate::ast::Item::Stmt(_)
                | crate::ast::Item::Use(_) | crate::ast::Item::StructDef(_)
                | crate::ast::Item::TestDef(_) => {}
            }
        }
        Ok(())
    }

    pub async fn exec_block_pub(
        &self,
        stmts:   &[crate::ast::Stmt],
        env:     &mut Env,
        ctx:     Option<Rc<std::cell::RefCell<crate::value::BotCtx>>>,
        outputs: &mut Vec<BotOutput>,
    ) -> Result<Value, String> {
        match self.exec_block(stmts, env, ctx, outputs).await {
            Ok(v)  => Ok(v),
            Err(e) => Err(format!("{e:?}")),
        }
    }

    pub async fn eval_block_public(
        &self,
        stmts:   &[crate::ast::Stmt],
        env:     &mut Env,
        ctx:     Option<Rc<std::cell::RefCell<crate::value::BotCtx>>>,
        outputs: &mut Vec<BotOutput>,
    ) -> crate::error::GravResult<Value> {
        self.exec_block_pub(stmts, env, ctx, outputs).await
            .map_err(crate::error::GravError::Runtime)
    }

    pub async fn format_traceback(&self) -> String {
        let st = self.shared.lock().await;
        if st.call_stack.is_empty() {
            return String::new();
        }
        st.call_stack.iter()
            .rev()
            .map(|s| format!("  at {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub async fn run_tests(&self, prog: &crate::ast::Program) -> Vec<(String, Result<(), String>)> {
        let mut results = Vec::new();
        for item in &prog.items {
            let crate::ast::Item::TestDef(test) = item else { continue };
            if test.is_scenario {
                // Feature N12: scenario test
                let outcome = self.run_scenario_test(prog, &test.body).await;
                results.push((test.name.clone(), outcome));
            } else {
                let mut env = Env::new();
                let mut outputs = Vec::new();
                let outcome = match self.exec_block(&test.body, &mut env, None, &mut outputs).await {
                    Ok(_) | Err(self::exec::ExecErr::Return(_)) => Ok(()),
                    Err(self::exec::ExecErr::Err(e)) => Err(e.to_string()),
                    Err(_) => Ok(()),
                };
                results.push((test.name.clone(), outcome));
            }
        }
        results
    }

    async fn run_scenario_test(&self, prog: &crate::ast::Program, stmts: &[crate::ast::Stmt]) -> Result<(), String> {
        use crate::value::{BotCtx, UpdateKind};
        let mut collected_outputs: Vec<crate::value::BotOutput> = Vec::new();

        for stmt in stmts {
            match stmt {
                crate::ast::Stmt::Simulate { user_id, action } => {
                    let mut env = Env::new();
                    let uid = self.eval_expr(user_id, &mut env, None).await
                        .map_err(|e| e.to_string())?
                        .as_int().unwrap_or(0);
                    let (text, is_callback) = match action {
                        crate::ast::SimAction::Sends(expr) => {
                            let t = self.eval_expr(expr, &mut env, None).await
                                .map_err(|e| e.to_string())?.to_string();
                            (t, false)
                        }
                        crate::ast::SimAction::Clicks(expr) => {
                            let t = self.eval_expr(expr, &mut env, None).await
                                .map_err(|e| e.to_string())?.to_string();
                            (t, true)
                        }
                    };
                    let (cmd, args_vec) = if !is_callback && text.starts_with('/') {
                        let parts: Vec<&str> = text[1..].splitn(2, ' ').collect();
                        (Some(parts[0].to_string()), parts.get(1).map(|a| a.split_whitespace().map(String::from).collect()).unwrap_or_default())
                    } else {
                        (None, vec![])
                    };
                    let update_type = if is_callback { "callback" } else if cmd.is_some() { "command" } else { "message" };
                    let ctx = BotCtx {
                        room_id: 1, user_id: uid, username: "test".into(),
                        text: Some(text.clone()), message_id: 1,
                        command: cmd, args: args_vec,
                        callback_data: if is_callback { Some(text) } else { None },
                        callback_id: None, timestamp: 0, reaction: None,
                        file_url: None, file_size: None, duration: None,
                        is_dm: false, mention_text: None,
                        update_kind: if is_callback { UpdateKind::Callback } else { UpdateKind::Message },
                        user_lang: None, webhook_body: None, webhook_headers: None,
                        vote_option: None, forward_from: None, is_thread: false,
                        intent: None, platform: "test".into(),
                    };
                    match self.dispatch(prog, ctx, update_type).await {
                        Ok(outputs) => collected_outputs = outputs,
                        Err(e) => return Err(format!("dispatch error: {e}")),
                    }
                }
                crate::ast::Stmt::ExpectReply { check } => {
                    let last_text = collected_outputs.iter().filter_map(|o| match o {
                        crate::value::BotOutput::Send { text, .. } => Some(text.clone()),
                        crate::value::BotOutput::Keyboard { text, .. } => Some(text.clone()),
                        _ => None,
                    }).last().unwrap_or_default();
                    let mut env = Env::new();
                    match check {
                        crate::ast::ExpectCheck::Contains(expr) => {
                            let expected = self.eval_expr(expr, &mut env, None).await
                                .map_err(|e| e.to_string())?.to_string();
                            if !last_text.contains(&expected) {
                                return Err(format!("expected reply to contain '{}', got '{}'", expected, last_text));
                            }
                        }
                        crate::ast::ExpectCheck::Equals(expr) => {
                            let expected = self.eval_expr(expr, &mut env, None).await
                                .map_err(|e| e.to_string())?.to_string();
                            if last_text != expected {
                                return Err(format!("expected reply '{}', got '{}'", expected, last_text));
                            }
                        }
                        crate::ast::ExpectCheck::Matches(expr) => {
                            let pattern = self.eval_expr(expr, &mut env, None).await
                                .map_err(|e| e.to_string())?.to_string();
                            let re = regex::Regex::new(&pattern).map_err(|e| e.to_string())?;
                            if !re.is_match(&last_text) {
                                return Err(format!("expected reply matching '{}', got '{}'", pattern, last_text));
                            }
                        }
                    }
                }
                _ => {
                    // Execute other statements normally
                    let mut env = Env::new();
                    let mut outputs = Vec::new();
                    let _ = self.exec_block(std::slice::from_ref(stmt), &mut env, None, &mut outputs).await;
                }
            }
        }
        Ok(())
    }

    pub async fn exec_stmt_pub(
        &self,
        stmt:    &crate::ast::Stmt,
        env:     &mut Env,
        ctx:     Option<Rc<std::cell::RefCell<crate::value::BotCtx>>>,
        outputs: &mut Vec<BotOutput>,
    ) -> crate::error::GravResult<()> {
        self.exec_stmt(stmt, env, ctx, outputs).await
            .map(|_| ())
            .map_err(|e| match e {
                self::exec::ExecErr::Err(ge) => ge,
                other => crate::error::GravError::Runtime(format!("{other}")),
            })
    }
}
