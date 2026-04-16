/// Gravitix AST pretty-printer / formatter.
///
/// Produces canonical, indented Gravitix source from a parsed `Program`.
/// Usage:
///   let formatted = gravitix::fmt::format_program(&prog);

use crate::ast::*;
use crate::lexer::StrPart;

const INDENT: &str = "    "; // 4 spaces

pub fn format_program(prog: &Program) -> String {
    let mut out = String::new();
    let mut first = true;
    for item in &prog.items {
        if !first { out.push('\n'); }
        first = false;
        format_item(&mut out, item, 0);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Items
// ─────────────────────────────────────────────────────────────────────────────

fn format_item(out: &mut String, item: &Item, depth: usize) {
    match item {
        Item::FnDef(fd)      => format_fn(out, fd, depth),
        Item::Handler(h)     => format_handler(out, h, depth),
        Item::FlowDef(fd)    => format_flow(out, fd, depth),
        Item::StateDef(sd)   => format_state(out, sd, depth),
        Item::Every(e)       => format_every(out, e, depth),
        Item::At(a)          => format_at(out, a, depth),
        Item::Use(path)      => push_line(out, depth, &format!("use \"{path}\"")),
        Item::StructDef(sd)    => format_struct(out, sd, depth),
        Item::TestDef(td)      => format_test(out, td, depth),
        Item::Stmt(s)          => format_stmt(out, s, depth),
        Item::FsmDef(f)        => {
            push_line(out, depth, &format!("fsm {} {{", f.name));
            push_line(out, depth + 1, &format!("initial: {}", f.initial));
            for state in &f.states {
                push_line(out, depth + 1, &format!("state {} {{", state.name));
                if !state.on_enter.is_empty() {
                    push_line(out, depth + 2, "on_enter {");
                    for s in &state.on_enter { format_stmt(out, s, depth + 3); }
                    push_line(out, depth + 2, "}");
                }
                for fh in &state.handlers {
                    let trig = match &fh.trigger {
                        crate::ast::FsmTrigger::Command(c) => format!("/{c}"),
                        crate::ast::FsmTrigger::AnyMsg     => "msg".to_string(),
                        crate::ast::FsmTrigger::Other(t)   => t.clone(),
                    };
                    push_line(out, depth + 2, &format!("on {trig} {{"));
                    for s in &fh.body { format_stmt(out, s, depth + 3); }
                    push_line(out, depth + 2, "}");
                }
                push_line(out, depth + 1, "}");
            }
            push_line(out, depth, "}");
        }
        Item::PermDef(p) => {
            push_line(out, depth, &format!("permission {} {{", p.name));
            push_line(out, depth + 1, &format_expr(&p.cond));
            push_line(out, depth, "}");
        }
        Item::ScheduleDef(s) => {
            push_line(out, depth, &format!("schedule \"{}\" {{", s.cron));
            for stmt in &s.body { format_stmt(out, stmt, depth + 1); }
            push_line(out, depth, "}");
        }
        Item::HookDef(h) => {
            let when = match h.when { HookWhen::Before => "before", HookWhen::After => "after" };
            push_line(out, depth, &format!("hook {when} msg {{"));
            for stmt in &h.body { format_stmt(out, stmt, depth + 1); }
            push_line(out, depth, "}");
        }
        Item::PluginDef(p) => {
            push_line(out, depth, &format!("plugin \"{}\" {{", p.name));
            for (k, v) in &p.config {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            push_line(out, depth, "}");
        }
        Item::MetricsDef(m) => {
            push_line(out, depth, "metrics {");
            for def in &m.defs {
                let kind = match def.kind {
                    MetricKind::Counter   => "counter",
                    MetricKind::Gauge     => "gauge",
                    MetricKind::Histogram => "histogram",
                };
                push_line(out, depth + 1, &format!("{kind} {}", def.name));
            }
            push_line(out, depth, "}");
        }
        Item::AbTestItem(ab) => {
            push_line(out, depth, &format!("abtest \"{}\" {{", ab.name));
            push_line(out, depth + 1, "variant A {");
            for stmt in &ab.variant_a { format_stmt(out, stmt, depth + 2); }
            push_line(out, depth + 1, "}");
            push_line(out, depth + 1, "variant B {");
            for stmt in &ab.variant_b { format_stmt(out, stmt, depth + 2); }
            push_line(out, depth + 1, "}");
            push_line(out, depth, "}");
        }
        Item::LangDef(ld) => {
            push_line(out, depth, "lang {");
            for (locale, pairs) in &ld.locales {
                push_line(out, depth + 1, &format!("{locale}: {{"));
                for (k, v) in pairs {
                    push_line(out, depth + 2, &format!("{k}: {}", format_expr(v)));
                }
                push_line(out, depth + 1, "}");
            }
            push_line(out, depth, "}");
        }
        Item::EnumDef(ed) => {
            push_line(out, depth, &format!("enum {} {{", ed.name));
            for v in &ed.variants {
                if v.fields.is_empty() {
                    push_line(out, depth + 1, &format!("{},", v.name));
                } else {
                    let types = v.fields.iter().map(format_type).collect::<Vec<_>>().join(", ");
                    push_line(out, depth + 1, &format!("{}({}),", v.name, types));
                }
            }
            push_line(out, depth, "}");
        }
        Item::ImplBlock(ib) => {
            push_line(out, depth, &format!("impl {} {{", ib.type_name));
            for method in &ib.methods {
                format_fn(out, method, depth + 1);
            }
            push_line(out, depth, "}");
        }
        Item::QueueDef(qd) => {
            push_line(out, depth, &format!("queue \"{}\" {{", qd.name));
            for (k, v) in &qd.config {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            push_line(out, depth, "}");
        }
        Item::WatchDef(wd) => {
            push_line(out, depth, &format!("watch state.{} {{", wd.field));
            for s in &wd.body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Item::AdminDef(ad) => {
            push_line(out, depth, "admin {");
            for (k, v) in &ad.config {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            for sec in &ad.sections {
                push_line(out, depth + 1, &format!("section \"{}\" {{", sec.name));
                for (k, v) in &sec.config {
                    push_line(out, depth + 2, &format!("{k}: {}", format_expr(v)));
                }
                push_line(out, depth + 1, "}");
            }
            push_line(out, depth, "}");
        }
        Item::MiddlewareDef(md) => {
            let params = format_params(&md.params);
            push_line(out, depth, &format!("middleware {}({params}) {{", md.name));
            for s in &md.body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Item::IntentsDef(id) => {
            push_line(out, depth, "intents {");
            for (name, phrases) in &id.intents {
                let ps: Vec<String> = phrases.iter().map(|p| format!("\"{p}\"")).collect();
                push_line(out, depth + 1, &format!("{name}: [{}]", ps.join(", ")));
            }
            push_line(out, depth, "}");
        }
        Item::EntitiesDef(ed) => {
            push_line(out, depth, "entities {");
            for ent in &ed.entities {
                match &ent.kind {
                    EntityKind::Builtin => push_line(out, depth + 1, &format!("{}: builtin", ent.name)),
                    EntityKind::List(vals) => {
                        let vs: Vec<String> = vals.iter().map(|v| format!("\"{v}\"")).collect();
                        push_line(out, depth + 1, &format!("{}: [{}]", ent.name, vs.join(", ")));
                    }
                }
            }
            push_line(out, depth, "}");
        }
        Item::CircuitBreakerDef(cb) => {
            push_line(out, depth, &format!("circuit_breaker \"{}\" {{", cb.name));
            for (k, v) in &cb.config {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            push_line(out, depth, "}");
        }
        Item::CanaryDef(cd) => {
            push_line(out, depth, &format!("canary \"{}\" {{", cd.name));
            push_line(out, depth + 1, &format!("percent: {}", cd.percent));
            for h in &cd.handlers {
                format_handler(out, h, depth + 1);
            }
            push_line(out, depth, "}");
        }
        Item::MultiplatformDef(mp) => {
            push_line(out, depth, "multiplatform {");
            for (platform, config) in &mp.platforms {
                push_line(out, depth + 1, &format!("{platform} {{"));
                for (k, v) in config {
                    push_line(out, depth + 2, &format!("{k}: {}", format_expr(v)));
                }
                push_line(out, depth + 1, "}");
            }
            push_line(out, depth, "}");
        }
        Item::MigrationDef(md) => {
            push_line(out, depth, &format!("migration \"{}\" {{", md.name));
            for s in &md.body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Item::UsePkg(name) => {
            push_line(out, depth, &format!("use pkg \"{name}\""));
        }
        Item::WebhookDef(wd) => {
            push_line(out, depth, &format!("webhook \"{}\" {{", wd.path));
            for (k, v) in &wd.config {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            for (event, body) in &wd.handlers {
                push_line(out, depth + 1, &format!("on \"{event}\" {{"));
                for s in body { format_stmt(out, s, depth + 2); }
                push_line(out, depth + 1, "}");
            }
            push_line(out, depth, "}");
        }
        Item::PermissionsDef(pd) => {
            push_line(out, depth, "permissions {");
            push_line(out, depth + 1, "roles: {");
            for (role, perms) in &pd.roles {
                let ps: Vec<String> = perms.iter().map(|p| format!("\"{p}\"")).collect();
                push_line(out, depth + 2, &format!("{role}: [{}]", ps.join(", ")));
            }
            push_line(out, depth + 1, "}");
            push_line(out, depth + 1, &format!("default: \"{}\"", pd.default_role));
            push_line(out, depth, "}");
        }
        Item::RatelimitDef(rd) => {
            push_line(out, depth, "ratelimit {");
            for rule in &rd.rules {
                let scope = match &rule.scope {
                    crate::ast::RatelimitScope::Global => "global".to_string(),
                    crate::ast::RatelimitScope::PerUser => "per_user".to_string(),
                    crate::ast::RatelimitScope::Command(c) => c.clone(),
                };
                let unit = if rule.window_ms >= 86_400_000 { "day" }
                    else if rule.window_ms >= 3_600_000 { "hour" }
                    else if rule.window_ms >= 60_000 { "minute" }
                    else { "second" };
                push_line(out, depth + 1, &format!("{scope}: {} per {unit}", rule.count));
            }
            push_line(out, depth, "}");
        }
        Item::Import(path) => {
            push_line(out, depth, &format!("import \"{path}\""));
        }
        Item::TypeDefItem(td) => {
            let constraint = td.constraint.as_ref()
                .map(|c| format!(" where {}", format_expr(c)))
                .unwrap_or_default();
            push_line(out, depth, &format!("typedef {} = {}{}", td.name, td.base_type, constraint));
        }
    }
}

fn format_fn(out: &mut String, fd: &FnDef, depth: usize) {
    for dec in &fd.decorators {
        if dec.args.is_empty() {
            push_line(out, depth, &format!("@{}", dec.name));
        } else {
            let args_str = dec.args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            push_line(out, depth, &format!("@{}({args_str})", dec.name));
        }
    }
    let params = format_params(&fd.params);
    let ret = fd.ret.as_ref()
        .map(|t| format!(" -> {}", format_type(t)))
        .unwrap_or_default();
    push_line(out, depth, &format!("fn {}({params}){ret} {{", fd.name));
    for s in &fd.body { format_stmt(out, s, depth + 1); }
    push_line(out, depth, "}");
}

fn format_handler(out: &mut String, h: &Handler, depth: usize) {
    let trigger = format_trigger(&h.trigger);
    let guard = h.guard.as_ref()
        .map(|e| format!(" guard {}", format_expr(e)))
        .unwrap_or_default();
    let rl = h.ratelimit.as_ref()
        .map(|r| {
            let scope = match r.per {
                RateLimitScope::User   => "user",
                RateLimitScope::Room   => "room",
                RateLimitScope::Global => "global",
            };
            let cooldown = r.cooldown.as_ref()
                .map(|c| format!(" cooldown \"{c}\""))
                .unwrap_or_default();
            format!(" ratelimit {}/{} per {scope}{cooldown}", r.count, r.window_ms)
        })
        .unwrap_or_default();
    push_line(out, depth, &format!("on {trigger}{guard}{rl} {{"));
    for s in &h.body { format_stmt(out, s, depth + 1); }
    push_line(out, depth, "}");
}

fn format_flow(out: &mut String, fd: &FlowDef, depth: usize) {
    push_line(out, depth, &format!("flow {} {{", fd.name));
    for s in &fd.body { format_stmt(out, s, depth + 1); }
    push_line(out, depth, "}");
}

fn format_state(out: &mut String, sd: &StateDef, depth: usize) {
    push_line(out, depth, "state {");
    for field in &sd.fields {
        let ty = format_type(&field.ty);
        let def = field.default.as_ref()
            .map(|e| format!(" = {}", format_expr(e)))
            .unwrap_or_default();
        let scope_prefix = match &field.scope {
            StateScope::Global  => "",
            StateScope::PerUser => "per_user ",
            StateScope::PerRoom => "per_room ",
        };
        push_line(out, depth + 1, &format!("{scope_prefix}{}: {ty}{def},", field.name));
    }
    push_line(out, depth, "}");
}

fn format_every(out: &mut String, e: &EveryDef, depth: usize) {
    let unit = match e.unit {
        TimeUnit::Sec  => "sec",
        TimeUnit::Min  => "min",
        TimeUnit::Hour => "hour",
        TimeUnit::Day  => "day",
    };
    push_line(out, depth, &format!("every {} {} {{", e.amount, unit));
    for s in &e.body { format_stmt(out, s, depth + 1); }
    push_line(out, depth, "}");
}

fn format_at(out: &mut String, a: &AtDef, depth: usize) {
    push_line(out, depth, &format!("at \"{}\" {{", a.time));
    for s in &a.body { format_stmt(out, s, depth + 1); }
    push_line(out, depth, "}");
}

fn format_struct(out: &mut String, sd: &StructDef, depth: usize) {
    push_line(out, depth, &format!("struct {} {{", sd.name));
    for (name, ty) in &sd.fields {
        push_line(out, depth + 1, &format!("{name}: {},", format_type(ty)));
    }
    push_line(out, depth, "}");
}

fn format_test(out: &mut String, td: &TestDef, depth: usize) {
    push_line(out, depth, &format!("test \"{}\" {{", td.name));
    for s in &td.body { format_stmt(out, s, depth + 1); }
    push_line(out, depth, "}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Statements
// ─────────────────────────────────────────────────────────────────────────────

fn format_stmt(out: &mut String, stmt: &Stmt, depth: usize) {
    match stmt {
        Stmt::Let { name, ty, value } => {
            let ty_str = ty.as_ref().map(|t| format!(": {}", format_type(t))).unwrap_or_default();
            push_line(out, depth, &format!("let {name}{ty_str} = {}", format_expr(value)));
        }
        Stmt::Assign { target, value } => {
            push_line(out, depth, &format!("{} = {}", format_expr(target), format_expr(value)));
        }
        Stmt::CompoundAssign { target, op, value } => {
            let op_str = match op {
                BinOp::Add => "+=", BinOp::Sub => "-=",
                BinOp::Mul => "*=", BinOp::Div => "/=",
                BinOp::Rem => "%=", _ => "+=",
            };
            push_line(out, depth, &format!("{} {op_str} {}", format_expr(target), format_expr(value)));
        }
        Stmt::Emit(e) => {
            push_line(out, depth, &format!("emit {}", format_expr(e)));
        }
        Stmt::EmitBroadcast(e) => {
            push_line(out, depth, &format!("emit broadcast {}", format_expr(e)));
        }
        Stmt::EmitTo { target, msg } => {
            push_line(out, depth, &format!("emit_to({}, {})", format_expr(target), format_expr(msg)));
        }
        Stmt::Return(e) => {
            let val = e.as_ref().map(|e| format!(" {}", format_expr(e))).unwrap_or_default();
            push_line(out, depth, &format!("return{val}"));
        }
        Stmt::Break    => push_line(out, depth, "break"),
        Stmt::Continue => push_line(out, depth, "continue"),
        Stmt::If { cond, then, elif, else_ } => {
            push_line(out, depth, &format!("if {} {{", format_expr(cond)));
            for s in then { format_stmt(out, s, depth + 1); }
            for (ec, eb) in elif {
                push_line(out, depth, &format!("}} elif {} {{", format_expr(ec)));
                for s in eb { format_stmt(out, s, depth + 1); }
            }
            if let Some(eb) = else_ {
                push_line(out, depth, "} else {");
                for s in eb { format_stmt(out, s, depth + 1); }
            }
            push_line(out, depth, "}");
        }
        Stmt::While { cond, body } => {
            push_line(out, depth, &format!("while {} {{", format_expr(cond)));
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::For { var, iter, body } => {
            push_line(out, depth, &format!("for {var} in {} {{", format_expr(iter)));
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::Match { subject, arms } => {
            push_line(out, depth, &format!("match {} {{", format_expr(subject)));
            for arm in arms {
                let pat = format_pattern(&arm.pattern);
                if arm.body.len() == 1 {
                    push_line(out, depth + 1, &format!("{pat} => {}", format_stmt_inline(&arm.body[0])));
                } else {
                    push_line(out, depth + 1, &format!("{pat} => {{"));
                    for s in &arm.body { format_stmt(out, s, depth + 2); }
                    push_line(out, depth + 1, "}");
                }
            }
            push_line(out, depth, "}");
        }
        Stmt::RunFlow(name) => push_line(out, depth, &format!("run flow {name}")),
        Stmt::TryCatch { try_body, err_name, catch_body, finally_body } => {
            push_line(out, depth, "try {");
            for s in try_body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, &format!("}} catch {err_name} {{"));
            for s in catch_body { format_stmt(out, s, depth + 1); }
            if !finally_body.is_empty() {
                push_line(out, depth, "} finally {");
                for s in finally_body { format_stmt(out, s, depth + 1); }
            }
            push_line(out, depth, "}");
        }
        Stmt::Reply { reply_to, text } => {
            push_line(out, depth, &format!("reply {} {}", format_expr(reply_to), format_expr(text)));
        }
        Stmt::DeleteMsg(msg_id) => {
            push_line(out, depth, &format!("delete_msg {}", format_expr(msg_id)));
        }
        Stmt::AnswerCallback(e) => {
            let arg = e.as_ref().map(|e| format!(" {}", format_expr(e))).unwrap_or_default();
            push_line(out, depth, &format!("answer_callback{arg}"));
        }
        Stmt::SendKeyboard { text, buttons } => {
            push_line(out, depth, &format!("send_keyboard {} {}", format_expr(text), format_expr(buttons)));
        }
        Stmt::EditMsg { msg_id, text } => {
            push_line(out, depth, &format!("edit {}, {}", format_expr(msg_id), format_expr(text)));
        }
        Stmt::Expr(e) => push_line(out, depth, &format_expr(e)),
        Stmt::Transition(state) => push_line(out, depth, &format!("→ {state}")),
        Stmt::Assert { cond, msg } => {
            let msg_str = msg.as_ref().map(|m| format!(", {}", format_expr(m))).unwrap_or_default();
            push_line(out, depth, &format!("assert {}{msg_str}", format_expr(cond)));
        }
        Stmt::EmitRich { fields } => {
            push_line(out, depth, "emit rich {");
            for (k, v) in fields {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            push_line(out, depth, "}");
        }
        Stmt::RunFsm(name) => push_line(out, depth, &format!("run fsm {name}")),
        Stmt::Stop => push_line(out, depth, "stop"),
        Stmt::FederatedEmit { target, msg } => {
            push_line(out, depth, &format!("federated emit {} {}", format_expr(target), format_expr(msg)));
        }
        Stmt::AbTest(ab) => {
            push_line(out, depth, &format!("abtest \"{}\" {{", ab.name));
            push_line(out, depth + 1, "variant A {");
            for stmt in &ab.variant_a { format_stmt(out, stmt, depth + 2); }
            push_line(out, depth + 1, "}");
            push_line(out, depth + 1, "variant B {");
            for stmt in &ab.variant_b { format_stmt(out, stmt, depth + 2); }
            push_line(out, depth + 1, "}");
            push_line(out, depth, "}");
        }
        Stmt::LetDestructMap { fields, value } => {
            push_line(out, depth, &format!("let {{{}}} = {}", fields.join(", "), format_expr(value)));
        }
        Stmt::LetDestructList { items, rest, value } => {
            let mut parts: Vec<String> = items.clone();
            if let Some(r) = rest {
                parts.push(format!("...{r}"));
            }
            push_line(out, depth, &format!("let [{}] = {}", parts.join(", "), format_expr(value)));
        }
        Stmt::Defer { body } => {
            push_line(out, depth, "defer {");
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::Paginate { items, page_size, format_fn, title } => {
            let mut line_str = format!("paginate({}, {})", format_expr(items), format_expr(page_size));
            if format_fn.is_some() || title.is_some() {
                line_str.push_str(" with {");
                if let Some(f) = format_fn { line_str.push_str(&format!(" format: {}", format_expr(f))); }
                if let Some(t) = title { line_str.push_str(&format!(" title: {}", format_expr(t))); }
                line_str.push_str(" }");
            }
            push_line(out, depth, &line_str);
        }
        Stmt::Spawn { body } => {
            push_line(out, depth, "spawn {");
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::Embed { fields } => {
            push_line(out, depth, "embed {");
            for (k, v) in fields {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            push_line(out, depth, "}");
        }
        Stmt::Enqueue { queue_name, body } => {
            push_line(out, depth, &format!("enqueue \"{queue_name}\" {{"));
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::Fire { event, data } => {
            push_line(out, depth, &format!("fire {} {}", format_expr(event), format_expr(data)));
        }
        Stmt::Select { arms } => {
            push_line(out, depth, "select {");
            for arm in arms {
                let kind_str = match &arm.kind {
                    crate::ast::SelectKind::WaitMsg => "wait msg".to_string(),
                    crate::ast::SelectKind::WaitCallback(None) => "wait callback".to_string(),
                    crate::ast::SelectKind::WaitCallback(Some(p)) => format!("wait callback \"{p}\""),
                    crate::ast::SelectKind::Timeout(ms) => format!("timeout {}s", ms / 1000),
                };
                let guard_str = arm.guard.as_ref().map(|g| format!(" guard {}", format_expr(g))).unwrap_or_default();
                push_line(out, depth + 1, &format!("{kind_str}{guard_str} => {{"));
                for s in &arm.body { format_stmt(out, s, depth + 2); }
                push_line(out, depth + 1, "}");
            }
            push_line(out, depth, "}");
        }
        Stmt::Mock { target, body } => {
            push_line(out, depth, &format!("mock {target} {{"));
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::Validate { value, kind, or_body } => {
            let args_str = if kind.args.is_empty() {
                String::new()
            } else {
                format!("({})", kind.args.iter().map(format_expr).collect::<Vec<_>>().join(", "))
            };
            let or_str = if or_body.is_empty() { String::new() } else { " or { ... }".to_string() };
            push_line(out, depth, &format!("validate {} as {}{}{}", format_expr(value), kind.name, args_str, or_str));
        }
        Stmt::Batch { body } => {
            push_line(out, depth, "batch {");
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::UseMiddleware(name) => {
            push_line(out, depth, &format!("use middleware {name}"));
        }
        Stmt::Breakpoint => push_line(out, depth, "breakpoint"),
        Stmt::Debug { body } => {
            push_line(out, depth, "debug {");
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::Simulate { user_id, action } => {
            let act = match action {
                SimAction::Sends(e) => format!("sends {}", format_expr(e)),
                SimAction::Clicks(e) => format!("clicks {}", format_expr(e)),
            };
            push_line(out, depth, &format!("simulate user {} {act}", format_expr(user_id)));
        }
        Stmt::ExpectReply { check } => {
            let chk = match check {
                ExpectCheck::Contains(e) => format!("contains {}", format_expr(e)),
                ExpectCheck::Equals(e) => format!("equals {}", format_expr(e)),
                ExpectCheck::Matches(e) => format!("matches {}", format_expr(e)),
            };
            push_line(out, depth, &format!("expect_reply {chk}"));
        }
        Stmt::Table { config } => {
            push_line(out, depth, "table {");
            for (k, v) in config {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            push_line(out, depth, "}");
        }
        Stmt::Chart { config } => {
            push_line(out, depth, "chart {");
            for (k, v) in config {
                push_line(out, depth + 1, &format!("{k}: {}", format_expr(v)));
            }
            push_line(out, depth, "}");
        }
        Stmt::Stream { body } => {
            push_line(out, depth, "stream {");
            for s in body { format_stmt(out, s, depth + 1); }
            push_line(out, depth, "}");
        }
        Stmt::Wizard { output_var, steps } => {
            push_line(out, depth, &format!("wizard -> {output_var} {{"));
            for step in steps {
                if step.is_confirm {
                    push_line(out, depth + 1, &format!("confirm {}", format_expr(&step.prompt)));
                } else {
                    let ty = format_type(&step.ty);
                    push_line(out, depth + 1, &format!("ask {} -> {}: {ty}", format_expr(&step.prompt), step.var));
                }
            }
            push_line(out, depth, "}");
        }
    }
}

fn format_stmt_inline(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Expr(e) => format_expr(e),
        other => {
            let mut buf = String::new();
            format_stmt(&mut buf, other, 0);
            buf.trim().to_string()
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Expressions
// ─────────────────────────────────────────────────────────────────────────────

pub fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Int(n)   => n.to_string(),
        Expr::Float(f) => {
            let s = format!("{f}");
            if s.contains('.') { s } else { format!("{s}.0") }
        }
        Expr::Bool(b)  => b.to_string(),
        Expr::Null     => "null".to_string(),
        Expr::Str(parts) => {
            let inner: String = parts.iter().map(|p| match p {
                StrPart::Lit(s)  => s.replace('\\', "\\\\").replace('"', "\\\""),
                StrPart::Hole(e) => format!("{{{e}}}"),
            }).collect();
            format!("\"{inner}\"")
        }
        Expr::Var(n)   => n.clone(),
        Expr::Wait         => "wait msg".to_string(),
        Expr::WaitCallback => "wait callback".to_string(),
        Expr::Ctx      => "ctx".to_string(),
        Expr::StateRef => "state".to_string(),
        Expr::EnvVar(k) => format!("env(\"{k}\")"),

        Expr::Complex(re, im) => {
            if *im >= 0.0 { format!("{re}+{im}i") } else { format!("{re}{im}i") }
        }
        Expr::Unary { op, expr } => {
            let op_str = match op {
                UnaryOp::Neg    => "-",
                UnaryOp::Not    => "!",
                UnaryOp::BitNot => "~",
            };
            format!("{op_str}{}", format_expr(expr))
        }
        Expr::Binary { op, lhs, rhs } => {
            let op_str = match op {
                BinOp::Add => "+",  BinOp::Sub => "-",  BinOp::Mul => "*",
                BinOp::Div => "/",  BinOp::Rem => "%",  BinOp::Pow => "**",
                BinOp::Eq  => "==", BinOp::Ne  => "!=", BinOp::Lt  => "<",
                BinOp::Gt  => ">",  BinOp::Le  => "<=", BinOp::Ge  => ">=",
                BinOp::And => "&&", BinOp::Or  => "||",
                BinOp::RangeEx => "..",  BinOp::RangeIn => "..=",
                BinOp::NullCoalesce => "??",
                BinOp::BitAnd => "&",  BinOp::BitOr => "|",  BinOp::BitXor => "^",
                BinOp::Shl    => "<<", BinOp::Shr   => ">>",
            };
            format!("({} {op_str} {})", format_expr(lhs), format_expr(rhs))
        }
        Expr::Pipe { lhs, fn_name, try_ } => {
            let try_str = if *try_ { "?" } else { "" };
            format!("{} |> {fn_name}{try_str}", format_expr(lhs))
        }
        Expr::Call { name, args } => {
            let args_str = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("{name}({args_str})")
        }
        Expr::Method { object, method, args } => {
            let args_str = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("{}.{method}({args_str})", format_expr(object))
        }
        Expr::Field { object, field } => format!("{}.{field}", format_expr(object)),
        Expr::Index { object, index }  => format!("{}[{}]", format_expr(object), format_expr(index)),
        Expr::Slice { object, start, end } => {
            let s = start.as_ref().map(|e| format_expr(e)).unwrap_or_default();
            let e = end.as_ref().map(|e| format_expr(e)).unwrap_or_default();
            format!("{}[{s}:{e}]", format_expr(object))
        }
        Expr::List(items) => {
            let inner = items.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("[{inner}]")
        }
        Expr::Map(pairs) => {
            let inner = pairs.iter()
                .map(|(k, v)| format!("{}: {}", format_expr(k), format_expr(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{inner}}}")
        }
        Expr::Lambda { params, body } => {
            let ps = format_params(params);
            if body.len() == 1 {
                format!("fn({ps}) {{ {} }}", format_stmt_inline(&body[0]))
            } else {
                format!("fn({ps}) {{ … }}")
            }
        }
        Expr::StructLit { type_name, fields } => {
            let inner = fields.iter()
                .map(|(k, v)| format!("{k}: {}", format_expr(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{type_name} {{ {inner} }}")
        }
        Expr::Parallel(exprs) => {
            let inner = exprs.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("parallel {{ {inner} }}")
        }
        Expr::Cache { key, ttl_secs, body } => {
            let body_str = body.iter().map(|s| {
                let mut buf = String::new();
                format_stmt(&mut buf, s, 1);
                buf
            }).collect::<String>();
            format!("cache({}, {}) {{{body_str}}}", format_expr(key), format_expr(ttl_secs))
        }
        Expr::OptionalField { object, field } => {
            format!("{}?.{field}", format_expr(object))
        }
        Expr::OptionalMethod { object, method, args } => {
            let args_str = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("{}?.{method}({args_str})", format_expr(object))
        }
        Expr::ListComp { expr, var, iter, cond } => {
            let cond_str = cond.as_ref()
                .map(|c| format!(" if {}", format_expr(c)))
                .unwrap_or_default();
            format!("[{} for {var} in {}{cond_str}]", format_expr(expr), format_expr(iter))
        }
        Expr::Try(inner) => format!("{}?", format_expr(inner)),
        Expr::WithBreaker { name, body } => {
            let body_str: String = body.iter().map(|s| {
                let mut buf = String::new();
                format_stmt(&mut buf, s, 1);
                buf
            }).collect();
            format!("with_breaker \"{name}\" {{{body_str}}}")
        }
        Expr::Sandbox { config } => {
            let inner = config.iter()
                .map(|(k, v)| format!("{k}: {}", format_expr(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("sandbox {{ {inner} }}")
        }
        Expr::Form { fields, submit } => {
            let mut parts = Vec::new();
            for f in fields {
                let kind = match &f.kind {
                    crate::ast::FormFieldKind::Text => "text",
                    crate::ast::FormFieldKind::Textarea => "textarea",
                    crate::ast::FormFieldKind::Number => "number",
                    crate::ast::FormFieldKind::Email => "email",
                    crate::ast::FormFieldKind::Phone => "phone",
                    crate::ast::FormFieldKind::Rating(_, _) => "rating",
                    crate::ast::FormFieldKind::Select(_) => "select",
                };
                let req = if f.required { " required" } else { "" };
                parts.push(format!("field \"{}\" {kind}{req}", f.name));
            }
            if let Some(s) = submit {
                parts.push(format!("submit \"{s}\""));
            }
            format!("form {{ {} }}", parts.join(", "))
        }
        Expr::WebSocket { url, config } => {
            let cfg = config.iter()
                .map(|(k, v)| format!("{k}: {}", format_expr(v)))
                .collect::<Vec<_>>()
                .join(", ");
            if cfg.is_empty() {
                format!("websocket {}", format_expr(url))
            } else {
                format!("websocket {} {{ {cfg} }}", format_expr(url))
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn format_trigger_pub(t: &Trigger) -> String {
    format_trigger(t)
}

fn format_trigger(t: &Trigger) -> String {
    match t {
        Trigger::Command(cmd)      => format!("/{cmd}"),
        Trigger::AnyMsg            => "msg".to_string(),
        Trigger::Callback(None)    => "callback".to_string(),
        Trigger::Callback(Some(s)) => format!("callback [\"{s}\"]"),
        Trigger::Join              => "join".to_string(),
        Trigger::Leave             => "leave".to_string(),
        Trigger::EditedMsg         => "edited".to_string(),
        Trigger::Any               => "any".to_string(),
        Trigger::Error             => "error".to_string(),
        Trigger::Reaction(None)    => "reaction".to_string(),
        Trigger::Reaction(Some(e)) => format!("reaction \"{e}\""),
        Trigger::File              => "file".to_string(),
        Trigger::Image             => "image".to_string(),
        Trigger::VoiceMsg          => "voice_msg".to_string(),
        Trigger::Mention           => "mention".to_string(),
        Trigger::Dm                => "dm".to_string(),
        Trigger::Idle(ms)          => {
            if ms % 3_600_000 == 0 { format!("idle({}h)", ms / 3_600_000) }
            else if ms % 60_000 == 0 { format!("idle({}m)", ms / 60_000) }
            else { format!("idle({}s)", ms / 1_000) }
        }
        Trigger::Webhook(path)     => format!("webhook \"{path}\""),
        Trigger::PollVote          => "poll_vote".to_string(),
        Trigger::Thread            => "thread".to_string(),
        Trigger::Forward           => "forward".to_string(),
        Trigger::Event(name)       => format!("event \"{name}\""),
        Trigger::Intent(name)      => format!("intent \"{name}\""),
        Trigger::IntentUnknown     => "intent unknown".to_string(),
    }
}

fn format_params(params: &[Param]) -> String {
    params.iter().map(|p| {
        let ty  = p.ty.as_ref().map(|t| format!(": {}", format_type(t))).unwrap_or_default();
        let def = p.default.as_ref().map(|e| format!(" = {}", format_expr(e))).unwrap_or_default();
        format!("{}{ty}{def}", p.name)
    }).collect::<Vec<_>>().join(", ")
}

fn format_type(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Int              => "int".to_string(),
        TypeExpr::Float            => "float".to_string(),
        TypeExpr::Bool             => "bool".to_string(),
        TypeExpr::Str              => "str".to_string(),
        TypeExpr::Void             => "void".to_string(),
        TypeExpr::Any              => "any".to_string(),
        TypeExpr::List(inner)      => format!("list[{}]", format_type(inner)),
        TypeExpr::Map(k, v)        => format!("map[{}, {}]", format_type(k), format_type(v)),
        TypeExpr::Named(n)         => n.clone(),
        TypeExpr::Optional(inner)  => format!("{}?", format_type(inner)),
        TypeExpr::Result           => "Result".to_string(),
        TypeExpr::Complex          => "complex".to_string(),
    }
}

fn format_pattern(pat: &Pattern) -> String {
    match pat {
        Pattern::Lit(e)               => format_expr(e),
        Pattern::Regex { pattern, flags } => format!("/{pattern}/{flags}"),
        Pattern::Wild                 => "_".to_string(),
        Pattern::Bind { name, inner } => format!("{name} @ {}", format_pattern(inner)),
        Pattern::EnumDestruct { enum_name, variant, bindings } => {
            if bindings.is_empty() {
                format!("{enum_name}.{variant}")
            } else {
                format!("{enum_name}.{}({})", variant, bindings.join(", "))
            }
        }
    }
}

fn push_line(out: &mut String, depth: usize, line: &str) {
    for _ in 0..depth { out.push_str(INDENT); }
    out.push_str(line);
    out.push('\n');
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    /// Parse source, format it, parse the formatted output again.
    /// If neither parse panics, the formatter produces valid Gravitix.
    fn roundtrip(src: &str) {
        let tokens = Lexer::new(src).tokenize().unwrap();
        let prog = Parser::new(tokens).parse().unwrap();
        let formatted = format_program(&prog);
        let tokens2 = Lexer::new(&formatted).tokenize().unwrap();
        let _prog2 = Parser::new(tokens2).parse()
            .unwrap_or_else(|e| panic!("roundtrip failed for:\n{}\n\nformatted:\n{}\n\nerror: {:?}", src, formatted, e));
    }

    #[test]
    fn fmt_empty_fn() {
        roundtrip("fn greet() { }");
    }

    #[test]
    fn fmt_fn_with_params() {
        roundtrip("fn add(a: int, b: int) -> int { return a + b }");
    }

    #[test]
    fn fmt_handler_command() {
        roundtrip("on /start { emit \"hello\" }");
    }

    #[test]
    fn fmt_handler_msg() {
        roundtrip("on msg { emit \"echo\" }");
    }

    #[test]
    fn fmt_let() {
        roundtrip("fn t() { let x = 42 }");
    }

    #[test]
    fn fmt_let_typed() {
        roundtrip("fn t() { let x: int = 42 }");
    }

    #[test]
    fn fmt_if_else() {
        roundtrip("fn t() { if true { emit \"yes\" } else { emit \"no\" } }");
    }

    #[test]
    fn fmt_for_loop() {
        roundtrip("fn t() { for i in [1, 2, 3] { emit i } }");
    }

    #[test]
    fn fmt_while_loop() {
        roundtrip("fn t() { while true { break } }");
    }

    #[test]
    fn fmt_struct() {
        roundtrip("struct Point { x: int, y: int }");
    }

    #[test]
    fn fmt_enum() {
        roundtrip("enum Color { Red, Green, Blue }");
    }

    #[test]
    fn fmt_state() {
        roundtrip("state { count: int = 0 }");
    }

    #[test]
    fn fmt_import() {
        roundtrip("import \"utils.grav\"");
    }

    #[test]
    fn fmt_try_catch() {
        roundtrip("fn t() { try { emit \"ok\" } catch e { emit \"err\" } }");
    }

    #[test]
    fn fmt_test_block() {
        roundtrip("test \"math\" { assert 2 + 2 == 4 }");
    }

    #[test]
    fn fmt_flow() {
        roundtrip("flow my_flow { emit \"step 1\" }");
    }

    #[test]
    fn fmt_use_path() {
        roundtrip("use \"utils.grav\"");
    }

    #[test]
    fn fmt_decorator_fn() {
        roundtrip("@retry(3)\nfn fetch() { }");
    }

    #[test]
    fn fmt_emit_broadcast() {
        roundtrip("fn t() { emit broadcast \"hi\" }");
    }

    #[test]
    fn fmt_match() {
        roundtrip("fn t() { match x { 1 => { emit \"one\" } } }");
    }

    #[test]
    fn fmt_format_program_not_empty() {
        let tokens = Lexer::new("fn hello() { emit \"hi\" }").tokenize().unwrap();
        let prog = Parser::new(tokens).parse().unwrap();
        let formatted = format_program(&prog);
        assert!(!formatted.is_empty());
        assert!(formatted.contains("fn hello"));
        assert!(formatted.contains("emit"));
    }
}
