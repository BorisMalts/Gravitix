use crate::lexer::StrPart;

// ─────────────────────────────────────────────────────────────────────────────
// Top-level program
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Decorator — `@name` or `@name(args)`
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Decorator {
    pub name: String,
    pub args: Vec<Expr>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Top-level items
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Item {
    /// `fn name(params) -> RetType { body }`
    FnDef(FnDef),

    /// `on /cmd guard cond { body }`  or  `on msg { body }`
    Handler(Handler),

    /// `flow name { body }`
    FlowDef(FlowDef),

    /// `state { field: Type = default, … }`
    StateDef(StateDef),

    /// `every <duration> { body }`
    Every(EveryDef),

    /// `at "HH:MM" { body }`
    At(AtDef),

    /// `use "path/to/file.grav"` — include another script
    Use(String),

    /// `struct Foo { x: int, y: str }`
    StructDef(StructDef),

    /// `test "name" { body }` — test block (run via `gravitix test`)
    TestDef(TestDef),

    /// `fsm Name { initial: state; state s { ... } }` — finite state machine
    FsmDef(FsmDef),

    /// `permission name { cond }` — permission definition
    PermDef(PermDef),

    /// `schedule "cron" { body }` — cron-based scheduler
    ScheduleDef(ScheduleDef),

    /// A standalone statement at module level (let, expr, …)
    Stmt(Stmt),

    /// `hook before/after msg { body }` — middleware
    HookDef(HookDef),

    /// `plugin "name" { config }` — plugin loader
    PluginDef(PluginDef),

    /// `metrics { counter x, gauge y, … }` — Prometheus metrics
    MetricsDef(MetricsDef),

    /// `abtest "name" { variant A { } variant B { } }` — A/B testing
    AbTestItem(AbTestDef),

    /// `lang { ru: { ... }, en: { ... } }` — i18n strings (Feature 12)
    LangDef(LangDef),

    /// `enum Status { Pending, Active, Banned(str) }` — user-defined enum
    EnumDef(EnumDef),

    /// `impl TypeName { fn method(self) { ... } }` — methods on structs
    ImplBlock(ImplBlock),

    /// `queue "name" { concurrency: 3, retry: 2 }` — job queue definition
    QueueDef(QueueDef),

    /// `watch state.field { body }` — reactive state watching (Feature 3)
    WatchDef(WatchDef),

    /// `admin { ... }` — auto-generated admin panel (Feature 9)
    AdminDef(AdminDef),

    /// `middleware name(params) { body }` — middleware definition (Feature 11)
    MiddlewareDef(MiddlewareDef),

    /// `intents { name: [phrases], ... }` — NLU intent definitions (Feature N1)
    IntentsDef(IntentsDef),

    /// `entities { name: builtin|[list], ... }` — entity extraction defs (Feature N2)
    EntitiesDef(EntitiesDef),

    /// `circuit_breaker "name" { config }` — circuit breaker definition (Feature N3)
    CircuitBreakerDef(CircuitBreakerDef),

    /// `canary "name" { percent: N, on trigger { } }` — canary deploy (Feature N5)
    CanaryDef(CanaryDef),

    /// `multiplatform { platform: { config }, ... }` — multi-platform bot (Feature N10)
    MultiplatformDef(MultiplatformDef),

    /// `migration "name" { body }` — data migration (Feature N11)
    MigrationDef(MigrationDef),

    /// `use pkg "name"` — package import (Feature N9)
    UsePkg(String),

    /// `webhook "/path" { config, on "event" { body } }` — incoming webhook (Feature W5)
    #[allow(dead_code)]
    WebhookDef(WebhookDef),

    /// `permissions { roles: { ... }, default: "role" }` — RBAC (Feature W7)
    #[allow(dead_code)]
    PermissionsDef(PermissionsDef),

    /// `ratelimit { global: N per minute, ... }` — granular rate limiting (Feature W8)
    #[allow(dead_code)]
    RatelimitDef(RatelimitDef),

    /// `import "file.grav"` — module import (Feature W9)
    Import(String),

    /// `typedef Name = base_type [where expr]` — type alias with validation (Feature W11)
    #[allow(dead_code)]
    TypeDefItem(TypeDefItem),
}

// ─────────────────────────────────────────────────────────────────────────────
// Enum definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name:     String,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name:   String,
    pub fields: Vec<TypeExpr>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Impl block — methods on structs
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub type_name: String,
    pub methods:   Vec<FnDef>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Queue definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct QueueDef {
    pub name:   String,
    pub config: Vec<(String, Expr)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// FSM definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FsmDef {
    pub name:    String,
    pub initial: String,
    pub states:  Vec<FsmState>,
}

#[derive(Debug, Clone)]
pub struct FsmState {
    pub name:     String,
    pub on_enter: Vec<Stmt>,
    pub on_leave: Vec<Stmt>,
    pub handlers: Vec<FsmHandler>,
}

#[derive(Debug, Clone)]
pub struct FsmHandler {
    pub trigger: FsmTrigger,
    pub body:    Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum FsmTrigger {
    /// `/command`
    Command(String),
    /// `msg` — any message
    AnyMsg,
    /// Any other trigger string
    Other(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Permission definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PermDef {
    pub name: String,
    pub cond: Expr,
}

// ─────────────────────────────────────────────────────────────────────────────
// Schedule definition (cron)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScheduleDef {
    pub cron: String,
    pub body: Vec<Stmt>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Struct definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name:   String,
    pub fields: Vec<(String, TypeExpr)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Test block  `test "name" { body }`
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TestDef {
    pub name: String,
    pub body: Vec<Stmt>,
    #[allow(dead_code)]
    pub line: u32,
    /// Whether this is a scenario test (Feature N12)
    #[allow(dead_code)]
    pub is_scenario: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Function definition
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name:       String,
    pub params:     Vec<Param>,
    pub ret:        Option<TypeExpr>,
    pub body:       Vec<Stmt>,
    /// Decorators applied to this function (Feature 1 new)
    pub decorators: Vec<Decorator>,
    /// Source line where this function was defined (for error messages)
    #[allow(dead_code)]
    pub line:       u32,
    /// Doc comment (Feature N7)
    #[allow(dead_code)]
    pub doc:        Option<String>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name:    String,
    pub ty:      Option<TypeExpr>,
    /// `fn foo(x: int = 0)` — default expression evaluated at call-time
    pub default: Option<Expr>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler  `on <trigger> [guard <expr>] { body }`
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RateLimit {
    pub count:     u32,
    pub window_ms: u64,
    pub per:       RateLimitScope,
    pub cooldown:  Option<String>,
}

#[derive(Debug, Clone)]
pub enum RateLimitScope { User, Room, Global }

#[derive(Debug, Clone)]
pub struct Handler {
    pub trigger:   Trigger,
    pub guard:     Option<Expr>,
    pub ratelimit: Option<RateLimit>,
    /// Permission name required to execute this handler
    #[allow(dead_code)]
    pub require:   Option<String>,
    pub body:      Vec<Stmt>,
    #[allow(dead_code)]
    pub line:      u32,
    /// Doc comment (Feature N7)
    #[allow(dead_code)]
    pub doc:       Option<String>,
}

#[derive(Debug, Clone)]
pub enum Trigger {
    /// `/start`  — slash command
    Command(String),
    /// `msg`     — any text message
    AnyMsg,
    /// `callback ["prefix"]` — inline button press, optional data prefix filter
    Callback(Option<String>),
    /// `join` — user joined room
    Join,
    /// `leave` — user left room
    Leave,
    /// `edited` — message was edited
    EditedMsg,
    /// catch-all
    Any,
    /// `error` — global error handler
    Error,
    /// `reaction "emoji"` or `reaction` (any reaction)
    Reaction(Option<String>),
    /// `file` — file upload
    File,
    /// `image` — image upload
    Image,
    /// `voice_msg` — voice message
    VoiceMsg,
    /// `mention` — bot was mentioned
    Mention,
    /// `dm` — direct message
    Dm,
    /// `idle(ms)` — user inactivity
    Idle(u64),
    /// `webhook "/path"` — HTTP webhook endpoint (Feature 10)
    #[allow(dead_code)]
    Webhook(String),
    /// `poll_vote` — poll vote event
    #[allow(dead_code)]
    PollVote,
    /// `thread` — thread reply event
    #[allow(dead_code)]
    Thread,
    /// `forward` — forwarded message event
    #[allow(dead_code)]
    Forward,
    /// `event "name"` — custom event trigger (Feature 2)
    #[allow(dead_code)]
    Event(String),
    /// `intent "name"` — NLU intent trigger (Feature N1)
    #[allow(dead_code)]
    Intent(String),
    /// `intent unknown` — unknown intent trigger (Feature N1)
    #[allow(dead_code)]
    IntentUnknown,
}

// ─────────────────────────────────────────────────────────────────────────────
// Flow (multi-step dialogue state machine)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FlowDef {
    pub name: String,
    pub body: Vec<Stmt>,
    #[allow(dead_code)]
    pub line: u32,
    /// Doc comment (Feature N7)
    #[allow(dead_code)]
    pub doc:  Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Persistent bot state
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StateDef {
    pub fields: Vec<StateField>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateScope {
    Global,
    PerUser,
    PerRoom,
}

#[derive(Debug, Clone)]
pub struct StateField {
    pub name:    String,
    pub ty:      TypeExpr,
    pub default: Option<Expr>,
    pub scope:   StateScope,
}

// ─────────────────────────────────────────────────────────────────────────────
// Schedulers
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EveryDef {
    pub amount: u64,
    pub unit:   TimeUnit,
    pub body:   Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct AtDef {
    pub time: String, // "HH:MM"
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum TimeUnit { Sec, Min, Hour, Day }

// ─────────────────────────────────────────────────────────────────────────────
// Hook definition  `hook before/after msg { body }`
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HookDef {
    pub when: HookWhen,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HookWhen { Before, After }

// ─────────────────────────────────────────────────────────────────────────────
// Plugin definition  `plugin "name" { key: expr, … }`
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PluginDef {
    pub name:   String,
    pub config: Vec<(String, Expr)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Metrics definition  `metrics { counter x, gauge y, … }`
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MetricsDef {
    pub defs: Vec<MetricDef>,
}

#[derive(Debug, Clone)]
pub struct MetricDef {
    pub kind: MetricKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetricKind { Counter, Gauge, Histogram }

// ─────────────────────────────────────────────────────────────────────────────
// A/B test definition  `abtest "name" { variant A { } variant B { } }`
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AbTestDef {
    pub name:      String,
    pub variant_a: Vec<Stmt>,
    pub variant_b: Vec<Stmt>,
}

// ─────────────────────────────────────────────────────────────────────────────
// i18n lang definition (Feature 12)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LangDef {
    pub locales: Vec<(String, Vec<(String, Expr)>)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Statements
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let name [: Type] = expr`
    Let { name: String, ty: Option<TypeExpr>, value: Expr },

    /// `name = expr`   or   `name.field = expr`  or  `name[i] = expr`
    Assign { target: Expr, value: Expr },

    /// `name op= expr`
    CompoundAssign { target: Expr, op: BinOp, value: Expr },

    /// `emit expr`
    Emit(Expr),

    /// `emit broadcast expr` — send to all known rooms
    EmitBroadcast(Expr),

    /// `emit_to(room_id, expr)` — отправить в конкретную комнату
    EmitTo { target: Expr, msg: Expr },

    /// `reply ctx.message_id "text"` — ответить на конкретное сообщение
    Reply { reply_to: Expr, text: Expr },

    /// `delete_msg ctx.message_id` — удалить сообщение
    DeleteMsg(Expr),

    /// `return [expr]`
    Return(Option<Expr>),

    /// `break`
    Break,

    /// `continue`
    Continue,

    /// `if cond { … } [elif cond { … }]* [else { … }]`
    If { cond: Expr, then: Vec<Stmt>, elif: Vec<(Expr, Vec<Stmt>)>, else_: Option<Vec<Stmt>> },

    /// `while cond { … }`
    While { cond: Expr, body: Vec<Stmt> },

    /// `for name in expr { … }`
    For { var: String, iter: Expr, body: Vec<Stmt> },

    /// `match expr { pat => stmt, … }`
    Match { subject: Expr, arms: Vec<MatchArm> },

    /// `run flow <name>`
    RunFlow(String),

    /// `try { … } catch name { … } [finally { … }]`
    TryCatch {
        try_body:     Vec<Stmt>,
        err_name:     String,
        catch_body:   Vec<Stmt>,
        finally_body: Vec<Stmt>,
    },

    /// `table { columns: [...], rows: expr, page_size: N }` — interactive table (Feature W2)
    #[allow(dead_code)]
    Table { config: Vec<(String, Expr)> },

    /// `chart { type: "bar", data: expr, ... }` — ASCII chart (Feature W3)
    #[allow(dead_code)]
    Chart { config: Vec<(String, Expr)> },

    /// `stream { body }` — streaming responses (Feature W6)
    #[allow(dead_code)]
    Stream { body: Vec<Stmt> },

    /// `keyboard "text", [[label, data], …]`
    SendKeyboard { text: Expr, buttons: Expr },

    /// `edit msg_id, "new text"`
    EditMsg { msg_id: Expr, text: Expr },

    /// `answer ["optional popup text"]`
    AnswerCallback(Option<Expr>),

    /// `wizard → var { ask ... }` — multi-step form
    Wizard { output_var: String, steps: Vec<WizardStep> },

    /// `→ state_name` — FSM state transition
    Transition(String),

    /// `assert expr` or `assert expr, "msg"` — assertion
    Assert { cond: Expr, msg: Option<Expr> },

    /// `emit rich { key: val, … }` — structured rich message
    EmitRich { fields: Vec<(String, Expr)> },

    /// `run fsm Name` — start FSM for current user
    RunFsm(String),

    /// Bare expression statement (function call, etc.)
    Expr(Expr),

    /// `stop` — stop handler chain (used in hooks)
    Stop,

    /// `federated emit "room@node" msg`
    FederatedEmit { target: Expr, msg: Expr },

    /// `abtest "name" { variant A { } variant B { } }` — inline A/B test statement
    AbTest(AbTestDef),

    /// `let {name, age} = expr` — map destructuring (Feature 1)
    LetDestructMap { fields: Vec<String>, value: Expr },

    /// `let [first, second, ...rest] = expr` — list destructuring (Feature 1)
    LetDestructList { items: Vec<String>, rest: Option<String>, value: Expr },

    /// `defer { body }` — deferred cleanup (Feature 4)
    Defer { body: Vec<Stmt> },

    /// `paginate(items, page_size) [with { format: fn, title: "..." }]` (Feature 11)
    Paginate {
        items: Expr,
        page_size: Expr,
        format_fn: Option<Expr>,
        title: Option<Expr>,
    },

    /// `spawn { body }` — background task
    Spawn { body: Vec<Stmt> },

    /// `embed { html: "...", height: 300, title: "..." }` — mini-app widget
    Embed { fields: Vec<(String, Expr)> },

    /// `enqueue "queue_name" { body }` — enqueue a job
    Enqueue { queue_name: String, body: Vec<Stmt> },

    /// `fire "event_name" data` — fire a custom event (Feature 2)
    Fire { event: Expr, data: Expr },

    /// `select { arms }` — multi-wait (Feature 4)
    #[allow(dead_code)]
    Select { arms: Vec<SelectArm> },

    /// `mock target { body }` — mock a function in test scope (Feature 5)
    Mock { target: String, body: Vec<Stmt> },

    /// `validate expr as kind or { body }` — data validation (Feature 6)
    Validate { value: Expr, kind: ValidateKind, or_body: Vec<Stmt> },

    /// `batch { body }` — group outputs (Feature 8)
    Batch { body: Vec<Stmt> },

    /// `use middleware name` — activate a middleware (Feature 11)
    #[allow(dead_code)]
    UseMiddleware(String),

    /// `breakpoint` — debugging: print all vars (Feature N8)
    Breakpoint,

    /// `debug { body }` — debugging: print exprs with line info (Feature N8)
    Debug { body: Vec<Stmt> },

    /// `simulate user(id) sends/clicks expr` — scenario test (Feature N12)
    Simulate { user_id: Expr, action: SimAction },

    /// `expect_reply contains/equals/matches expr` — scenario test (Feature N12)
    ExpectReply { check: ExpectCheck },
}

// ─────────────────────────────────────────────────────────────────────────────
// Wizard step
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WizardStep {
    pub prompt:     Expr,
    pub var:        String,
    pub ty:         TypeExpr,
    pub validate:   Option<Expr>,
    pub is_confirm: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Match arm
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body:    Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    /// Literal: "bye"  42  true  null
    Lit(Expr),
    /// Regex:  /hello|hi/i
    Regex { pattern: String, flags: String },
    /// Wildcard: `_`
    Wild,
    /// Binding: `name @ pattern`
    Bind { name: String, inner: Box<Pattern> },
    /// Enum destructuring: `Status.Banned(reason)` or Result patterns `Ok(val)`, `Err(e)`
    EnumDestruct { enum_name: String, variant: String, bindings: Vec<String> },
}

// ─────────────────────────────────────────────────────────────────────────────
// Expressions
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Str(Vec<StrPart>),

    /// Complex literal: pure imaginary `5i` → Complex(0.0, 5.0)
    /// Full complex `3+2i` is parsed as Binary(Add, Float(3), Complex(0, 2))
    Complex(f64, f64),

    // Variable
    Var(String),

    // `wait msg` — suspend flow and wait for next message
    Wait,

    // `wait callback` — suspend flow until inline button is pressed
    WaitCallback,

    // `env("KEY")` — read environment variable
    EnvVar(String),

    // Unary
    Unary { op: UnaryOp, expr: Box<Expr> },

    // Binary
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },

    // Pipe:  expr |> fn  (try_: true if `fn?` syntax used)
    Pipe { lhs: Box<Expr>, fn_name: String, try_: bool },

    // Call:  name(args)
    Call { name: String, args: Vec<Expr> },

    // Method call:  expr.method(args)
    Method { object: Box<Expr>, method: String, args: Vec<Expr> },

    // Field access:  expr.field
    Field { object: Box<Expr>, field: String },

    // Index:  expr[idx]
    Index { object: Box<Expr>, index: Box<Expr> },

    // List literal:  [a, b, c]
    List(Vec<Expr>),

    // Map literal:  { "key": val, … }
    Map(Vec<(Expr, Expr)>),

    // `ctx`  — the current bot context (injected implicitly)
    Ctx,

    // `state`  — the bot's global state object
    StateRef,

    // `fn(params) { body }` — anonymous function (closure)
    Lambda { params: Vec<Param>, body: Vec<Stmt> },

    // `expr[start:end]` — slice of list or string
    Slice { object: Box<Expr>, start: Option<Box<Expr>>, end: Option<Box<Expr>> },

    // `TypeName { field: val, … }` — struct literal (stored as Map with __type__)
    StructLit { type_name: String, fields: Vec<(String, Expr)> },

    /// `parallel { expr, expr, ... }` — run expressions concurrently
    Parallel(Vec<Expr>),

    /// `cache("key", ttl) { body }` — TTL cache block
    Cache { key: Box<Expr>, ttl_secs: Box<Expr>, body: Vec<Stmt> },

    /// `expr?.field` — optional chaining field access (Feature 3)
    OptionalField { object: Box<Expr>, field: String },

    /// `expr?.method(args)` — optional chaining method call (Feature 3)
    OptionalMethod { object: Box<Expr>, method: String, args: Vec<Expr> },

    /// `[expr for var in iter]` or `[expr for var in iter if cond]` (Feature 2)
    ListComp { expr: Box<Expr>, var: String, iter: Box<Expr>, cond: Option<Box<Expr>> },

    /// `expr?` — try/unwrap Result type (Feature 6)
    Try(Box<Expr>),

    /// `sandbox { config }` — isolated code execution (Feature 12)
    Sandbox { config: Vec<(String, Expr)> },

    /// `with_breaker "name" { body }` — circuit breaker execution (Feature N3)
    WithBreaker { name: String, body: Vec<Stmt> },

    /// `form { field "name" text required, ... submit "label" }` — interactive form (Feature W1)
    #[allow(dead_code)]
    Form { fields: Vec<FormField>, submit: Option<String> },

    /// `websocket "url" { on_message: fn, ... }` — WebSocket client stub (Feature W4)
    #[allow(dead_code)]
    WebSocket { url: Box<Expr>, config: Vec<(String, Expr)> },
}

// ─────────────────────────────────────────────────────────────────────────────
// Operators
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Rem, Pow,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or,
    RangeEx, RangeIn,
    /// `??` — null coalescing (Feature 3)
    NullCoalesce,
    /// Bitwise operators
    BitAnd, BitOr, BitXor, Shl, Shr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp { Neg, Not, BitNot }

// ─────────────────────────────────────────────────────────────────────────────
// Type expressions
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Int, Float, Bool, Str, Void,
    /// Explicit `any` type — accepts any value without type-check
    Any,
    /// Complex number type (re + im*i)
    Complex,
    List(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    Named(String),
    Optional(Box<TypeExpr>),
    /// `Result` type for Feature 6
    Result,
}

// ─────────────────────────────────────────────────────────────────────────────
// Select arm (Feature 4)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SelectArm {
    pub kind:  SelectKind,
    pub guard: Option<Expr>,
    pub body:  Vec<Stmt>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum SelectKind {
    WaitMsg,
    WaitCallback(Option<String>),
    Timeout(u64),
}

// ─────────────────────────────────────────────────────────────────────────────
// Validate kind (Feature 6)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ValidateKind {
    pub name: String,
    pub args: Vec<Expr>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Watch definition (Feature 3)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WatchDef {
    pub field: String,
    pub body:  Vec<Stmt>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Admin definition (Feature 9)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminDef {
    pub config:   Vec<(String, Expr)>,
    pub sections: Vec<AdminSection>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminSection {
    pub name:   String,
    pub config: Vec<(String, Expr)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Middleware definition (Feature 11)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MiddlewareDef {
    pub name:   String,
    pub params: Vec<Param>,
    pub body:   Vec<Stmt>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Intents definition (Feature N1)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IntentsDef {
    pub intents: Vec<(String, Vec<String>)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Entity definition (Feature N2)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EntitiesDef {
    pub entities: Vec<EntityDef>,
}

#[derive(Debug, Clone)]
pub struct EntityDef {
    pub name: String,
    pub kind: EntityKind,
}

#[derive(Debug, Clone)]
pub enum EntityKind {
    Builtin,
    List(Vec<String>),
}

// ─────────────────────────────────────────────────────────────────────────────
// Circuit breaker definition (Feature N3)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CircuitBreakerDef {
    pub name:   String,
    pub config: Vec<(String, Expr)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Canary definition (Feature N5)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CanaryDef {
    pub name:     String,
    pub percent:  u8,
    pub handlers: Vec<Handler>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Multiplatform definition (Feature N10)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MultiplatformDef {
    pub platforms: Vec<(String, Vec<(String, Expr)>)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Migration definition (Feature N11)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MigrationDef {
    pub name: String,
    pub body: Vec<Stmt>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Webhook definition (Feature W5)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WebhookDef {
    pub path:     String,
    pub config:   Vec<(String, Expr)>,
    pub handlers: Vec<(String, Vec<Stmt>)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Permissions RBAC definition (Feature W7)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PermissionsDef {
    pub roles:        Vec<(String, Vec<String>)>,
    pub default_role: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Ratelimit definition (Feature W8)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RatelimitDef {
    pub rules: Vec<RatelimitRule>,
}

#[derive(Debug, Clone)]
pub struct RatelimitRule {
    pub scope:     RatelimitScope,
    pub count:     u32,
    pub window_ms: u64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RatelimitScope {
    Global,
    PerUser,
    Command(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// TypeDef definition (Feature W11)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TypeDefItem {
    pub name:       String,
    pub base_type:  String,
    pub constraint: Option<Expr>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Form expression (Feature W1)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FormField {
    pub name:     String,
    pub kind:     FormFieldKind,
    pub required: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum FormFieldKind {
    Text,
    Textarea,
    Rating(i64, i64),
    Select(Vec<String>),
    Number,
    Email,
    Phone,
}

// ─────────────────────────────────────────────────────────────────────────────
// Scenario test statements (Feature N12)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum SimAction {
    Sends(Expr),
    Clicks(Expr),
}

#[derive(Debug, Clone)]
pub enum ExpectCheck {
    Contains(Expr),
    Equals(Expr),
    Matches(Expr),
}
