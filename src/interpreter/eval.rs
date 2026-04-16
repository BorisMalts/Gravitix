use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

use crate::ast::*;
use crate::error::{GravError, GravResult};
use crate::lexer::StrPart;
use crate::value::{BotCtx, Value};
use crate::{runtime_err, type_err};
use super::{Interpreter, Env};
use super::exec::ExecErr;

impl Interpreter {
    // ── evaluate an expression ────────────────────────────────────────────────

    pub async fn eval_expr(
        &self,
        expr: &Expr,
        env:  &mut Env,
        ctx:  Option<Rc<RefCell<BotCtx>>>,
    ) -> GravResult<Value> {
        match expr {
            Expr::Int(n)   => Ok(Value::Int(*n)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Bool(b)  => Ok(Value::Bool(*b)),
            Expr::Null     => Ok(Value::Null),

            Expr::Str(parts) => {
                let mut out = String::new();
                for part in parts {
                    match part {
                        StrPart::Lit(s)  => out.push_str(s),
                        StrPart::Hole(src) => {
                            // Re-lex and parse the hole expression
                            let v = Box::pin(self.eval_hole(src, env, ctx.clone())).await?;
                            out.push_str(&v.to_string());
                        }
                    }
                }
                Ok(Value::make_str(out))
            }

            Expr::Var(name) => {
                env.get(name).ok_or_else(|| {
                    let vars = env.all_var_names();
                    let suggestion = crate::error::did_you_mean(name, &vars)
                        .map(|s| format!(" (did you mean `{}`?)", s))
                        .unwrap_or_default();
                    GravError::UndefinedVar(format!("{}{}", name, suggestion))
                })
            }

            Expr::Ctx => {
                let c = ctx.ok_or_else(|| runtime_err!("'ctx' not available here"))?;
                Ok(Value::Ctx(c))
            }

            Expr::StateRef => {
                let st = self.shared.lock().await;
                let map: HashMap<String, Value> = st.bot_state.clone();
                Ok(Value::make_map(map))
            }

            Expr::EnvVar(key) => {
                let val = std::env::var(key).unwrap_or_default();
                Ok(Value::make_str(val))
            }

            Expr::Wait => {
                let (room_id, user_id) = {
                    let c = ctx.as_ref().ok_or_else(|| runtime_err!("'wait msg' needs ctx"))?;
                    let c = c.borrow();
                    (c.room_id, c.user_id)
                };
                let (tx, rx) = tokio::sync::oneshot::channel::<String>();
                {
                    let mut st = self.shared.lock().await;
                    st.wait_map.insert((room_id, user_id), tx);
                }
                match tokio::time::timeout(tokio::time::Duration::from_secs(300), rx).await {
                    Ok(Ok(text))  => Ok(Value::make_str(text)),
                    Ok(Err(_))    => Ok(Value::make_str("__cancelled__")),
                    Err(_elapsed) => {
                        self.shared.lock().await.wait_map.remove(&(room_id, user_id));
                        Ok(Value::make_str("__timeout__"))
                    }
                }
            }

            Expr::WaitCallback => {
                let (room_id, user_id) = {
                    let c = ctx.as_ref().ok_or_else(|| runtime_err!("'wait callback' needs ctx"))?;
                    let c = c.borrow();
                    (c.room_id, c.user_id)
                };
                let (tx, rx) = tokio::sync::oneshot::channel::<String>();
                {
                    let mut st = self.shared.lock().await;
                    st.callback_wait_map.insert((room_id, user_id), tx);
                }
                match tokio::time::timeout(tokio::time::Duration::from_secs(300), rx).await {
                    Ok(Ok(data))  => Ok(Value::make_str(data)),
                    Ok(Err(_))    => Ok(Value::make_str("__cancelled__")),
                    Err(_elapsed) => {
                        self.shared.lock().await.callback_wait_map.remove(&(room_id, user_id));
                        Ok(Value::make_str("__timeout__"))
                    }
                }
            }

            Expr::Complex(re, im) => Ok(Value::Complex(*re, *im)),

            Expr::Unary { op, expr } => {
                let v = Box::pin(self.eval_expr(expr, env, ctx)).await?;
                match op {
                    UnaryOp::Neg => match v {
                        Value::Int(n)   => Ok(Value::Int(-n)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        Value::Complex(r, i) => Ok(Value::Complex(-r, -i)),
                        _ => Err(type_err!("number", v.type_name())),
                    },
                    UnaryOp::Not => Ok(Value::Bool(!v.is_truthy())),
                    UnaryOp::BitNot => match v {
                        Value::Int(n) => Ok(Value::Int(!n)),
                        _ => Err(runtime_err!("bitwise NOT (~) requires integer")),
                    },
                }
            }

            Expr::Binary { op, lhs, rhs } => {
                // Short-circuit for && and ||
                match op {
                    BinOp::And => {
                        let l = Box::pin(self.eval_expr(lhs, env, ctx.clone())).await?;
                        if !l.is_truthy() { return Ok(Value::Bool(false)); }
                        let r = Box::pin(self.eval_expr(rhs, env, ctx)).await?;
                        return Ok(Value::Bool(r.is_truthy()));
                    }
                    BinOp::Or => {
                        let l = Box::pin(self.eval_expr(lhs, env, ctx.clone())).await?;
                        if l.is_truthy() { return Ok(Value::Bool(true)); }
                        let r = Box::pin(self.eval_expr(rhs, env, ctx)).await?;
                        return Ok(Value::Bool(r.is_truthy()));
                    }
                    _ => {}
                }
                let l = Box::pin(self.eval_expr(lhs, env, ctx.clone())).await?;
                let r = Box::pin(self.eval_expr(rhs, env, ctx)).await?;
                crate::value::apply_binop(op.clone(), l, r)
            }

            Expr::Pipe { lhs, fn_name, try_ } => {
                let val = Box::pin(self.eval_expr(lhs, env, ctx.clone())).await?;
                let result = self.call_fn(fn_name, vec![val], env, ctx).await?;
                if *try_ {
                    // If result is an Err result map, propagate
                    if let Value::Map(ref m) = result {
                        let m_ref = m.borrow();
                        if let Some(Value::Str(rt)) = m_ref.get("__result__") {
                            if rt.as_str() == "err" {
                                let err = m_ref.get("error").map(|v| v.to_string()).unwrap_or_default();
                                return Err(GravError::Runtime(format!("try? propagated error: {err}")));
                            } else if rt.as_str() == "ok" {
                                return Ok(m_ref.get("value").cloned().unwrap_or(Value::Null));
                            }
                        }
                    }
                }
                Ok(result)
            }

            Expr::Call { name, args } => {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for a in args {
                    evaluated_args.push(Box::pin(self.eval_expr(a, env, ctx.clone())).await?);
                }
                // Feature 9: chat action builtins that need ctx — return Value with side effect marker
                match name.as_str() {
                    "typing" | "pin_msg" | "unpin_msg" | "mute_user" => {
                        // Store a special marker that exec can interpret.
                        // We encode as a map with __action__
                        let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                        let mut m = HashMap::new();
                        m.insert("__action__".to_string(), Value::make_str(name.as_str()));
                        m.insert("room_id".to_string(), Value::Int(room_id));
                        for (i, arg) in evaluated_args.iter().enumerate() {
                            m.insert(format!("arg{i}"), arg.clone());
                        }
                        return Ok(Value::make_map(m));
                    }
                    _ => {}
                }
                self.call_fn(name, evaluated_args, env, ctx).await
            }

            Expr::Method { object, method, args } => {
                // Feature 1: enum tuple variant call (e.g. Status.Banned("spam"))
                if let Expr::Var(ref enum_name) = object.as_ref() {
                    let is_enum = {
                        let st = self.shared.lock().await;
                        st.enum_defs.contains_key(enum_name.as_str())
                    };
                    if is_enum {
                        let mut evaluated_args = Vec::with_capacity(args.len());
                        for a in args {
                            evaluated_args.push(Box::pin(self.eval_expr(a, env, ctx.clone())).await?);
                        }
                        let mut m = HashMap::new();
                        m.insert("__enum__".to_string(), Value::make_str(enum_name.as_str()));
                        m.insert("__variant__".to_string(), Value::make_str(method.as_str()));
                        for (i, arg) in evaluated_args.iter().enumerate() {
                            m.insert(format!("__{i}__"), arg.clone());
                        }
                        return Ok(Value::make_map(m));
                    }
                }
                // Feature 10: ctx.history(n) — return last N messages
                if let Expr::Ctx = object.as_ref() {
                    if method == "history" {
                        let n = if let Some(a) = args.first() {
                            Box::pin(self.eval_expr(a, env, ctx.clone())).await?
                                .as_int().unwrap_or(5) as usize
                        } else { 5 };
                        let (room_id, user_id) = ctx.as_ref()
                            .map(|c| { let c = c.borrow(); (c.room_id, c.user_id) })
                            .unwrap_or((0, 0));
                        let st = self.shared.lock().await;
                        let history = st.message_history.get(&(room_id, user_id))
                            .map(|deq| {
                                let skip = if deq.len() > n { deq.len() - n } else { 0 };
                                deq.iter().skip(skip).cloned().collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        return Ok(Value::make_list(history));
                    }
                }
                // Feature 5: http.get/post/put/delete method calls
                if let Expr::Var(ref obj_name) = object.as_ref() {
                    if obj_name == "http" && matches!(method.as_str(), "get" | "post" | "put" | "delete") {
                        let mut evaluated_args = Vec::with_capacity(args.len());
                        for a in args {
                            evaluated_args.push(Box::pin(self.eval_expr(a, env, ctx.clone())).await?);
                        }
                        let fn_name = format!("http_{method}");
                        if let Some(v) = crate::stdlib::call_builtin(&fn_name, &evaluated_args, &self.shared).await? {
                            return Ok(v);
                        }
                    }
                }
                // Feature 9: metrics.name access
                if let Expr::Var(ref obj_name) = object.as_ref() {
                    if obj_name == "metrics" {
                        let st = self.shared.lock().await;
                        let val = st.bot_metrics.get(method).copied().unwrap_or(0.0);
                        return Ok(Value::Float(val));
                    }
                }
                let obj = Box::pin(self.eval_expr(object, env, ctx.clone())).await?;
                let mut evaluated_args = Vec::with_capacity(args.len());
                for a in args {
                    evaluated_args.push(Box::pin(self.eval_expr(a, env, ctx.clone())).await?);
                }
                let result = self.call_method(obj, method, evaluated_args)?;
                // Feature 2: intercept __impl_call__ marker for async execution
                if let Value::Map(ref m) = result {
                    let m_ref = m.borrow();
                    if let Some(Value::Fn(fd)) = m_ref.get("__impl_call__") {
                        let fd = fd.clone();
                        let self_val = m_ref.get("__self__").cloned().unwrap_or(Value::Null);
                        drop(m_ref);
                        let mut fn_env = super::Env::new();
                        fn_env.define("self", self_val);
                        if let Some(c) = &ctx { fn_env.define("ctx", Value::Ctx(c.clone())); }
                        let param_start = if fd.params.first().map_or(false, |p| p.name == "self") { 1 } else { 0 };
                        // Re-evaluate args for the impl call
                        let mut impl_args = Vec::new();
                        for a in args {
                            impl_args.push(Box::pin(self.eval_expr(a, env, ctx.clone())).await?);
                        }
                        for (param, val) in fd.params.iter().skip(param_start).zip(impl_args) {
                            fn_env.define(&param.name, val);
                        }
                        let mut dummy_out = Vec::new();
                        return match Box::pin(self.exec_block(&fd.body, &mut fn_env, ctx.clone(), &mut dummy_out)).await {
                            Ok(v) => Ok(v),
                            Err(super::exec::ExecErr::Return(v)) => Ok(v),
                            Err(super::exec::ExecErr::Err(e)) => Err(e),
                            Err(_) => Ok(Value::Null),
                        };
                    }
                }
                Ok(result)
            }

            Expr::Field { object, field } => {
                // Feature 9: metrics.name field access
                if let Expr::Var(ref n) = object.as_ref() {
                    if n == "metrics" {
                        let st = self.shared.lock().await;
                        let v = st.bot_metrics.get(field.as_str()).copied().unwrap_or(0.0);
                        return Ok(Value::Float(v));
                    }
                }
                // Feature 1: enum unit variant access (e.g. Status.Active)
                if let Expr::Var(ref enum_name) = object.as_ref() {
                    let is_enum = {
                        let st = self.shared.lock().await;
                        st.enum_defs.contains_key(enum_name.as_str())
                    };
                    if is_enum {
                        let mut m = HashMap::new();
                        m.insert("__enum__".to_string(), Value::make_str(enum_name.as_str()));
                        m.insert("__variant__".to_string(), Value::make_str(field.as_str()));
                        return Ok(Value::make_map(m));
                    }
                }
                // Intercept state.field for per-user/per-room scope support
                if matches!(object.as_ref(), Expr::StateRef) {
                    let (user_id, room_id) = ctx.as_ref()
                        .map(|c| { let c = c.borrow(); (c.user_id, c.room_id) })
                        .unwrap_or((0, 0));
                    let st = self.shared.lock().await;
                    return Ok(st.get_state_field(field, user_id, room_id));
                }
                let obj = Box::pin(self.eval_expr(object, env, ctx.clone())).await?;
                match &obj {
                    Value::Ctx(c)  => Ok(c.borrow().get_field(field)),
                    Value::Map(m)  => Ok(m.borrow().get(field.as_str()).cloned().unwrap_or(Value::Null)),
                    _ => Err(runtime_err!("cannot access field '{}' on {}", field, obj.type_name())),
                }
            }

            Expr::Index { object, index } => {
                let obj = Box::pin(self.eval_expr(object, env, ctx.clone())).await?;
                let idx = Box::pin(self.eval_expr(index, env, ctx)).await?;
                match (&obj, &idx) {
                    (Value::List(l), Value::Int(i)) => {
                        let l = l.borrow();
                        let i = if *i < 0 { (l.len() as i64 + i) as usize } else { *i as usize };
                        Ok(l.get(i).cloned().unwrap_or(Value::Null))
                    }
                    (Value::Map(m), _) => {
                        let key = idx.to_string();
                        Ok(m.borrow().get(&key).cloned().unwrap_or(Value::Null))
                    }
                    _ => Err(type_err!("list or map", obj.type_name())),
                }
            }

            Expr::List(elems) => {
                let mut v = Vec::with_capacity(elems.len());
                for e in elems {
                    v.push(Box::pin(self.eval_expr(e, env, ctx.clone())).await?);
                }
                Ok(Value::make_list(v))
            }

            Expr::Map(pairs) => {
                let mut m = HashMap::new();
                for (k, v) in pairs {
                    let key = Box::pin(self.eval_expr(k, env, ctx.clone())).await?.to_string();
                    let val = Box::pin(self.eval_expr(v, env, ctx.clone())).await?;
                    m.insert(key, val);
                }
                Ok(Value::make_map(m))
            }

            Expr::Lambda { params, body } => {
                // Create a closure-like FnDef value
                use crate::ast::{FnDef};
                let fd = FnDef {
                    name:       "<lambda>".into(),
                    params:     params.clone(),
                    ret:        None,
                    body:       body.clone(),
                    decorators: vec![],
                    doc:        None,
                    line:       0,
                };
                Ok(Value::Fn(std::rc::Rc::new(fd)))
            }

            Expr::Slice { object, start, end } => {
                let obj = Box::pin(self.eval_expr(object, env, ctx.clone())).await?;
                let s = start.as_ref();
                let e = end.as_ref();
                match obj {
                    Value::List(l) => {
                        let l = l.borrow();
                        let len = l.len() as i64;
                        let start_idx = if let Some(s_expr) = s {
                            let v = Box::pin(self.eval_expr(s_expr, env, ctx.clone())).await?;
                            let i = v.as_int().unwrap_or(0);
                            if i < 0 { (len + i).max(0) as usize } else { i.min(len) as usize }
                        } else { 0 };
                        let end_idx = if let Some(e_expr) = e {
                            let v = Box::pin(self.eval_expr(e_expr, env, ctx.clone())).await?;
                            let i = v.as_int().unwrap_or(len);
                            if i < 0 { (len + i).max(0) as usize } else { i.min(len) as usize }
                        } else { len as usize };
                        let sliced = l[start_idx.min(end_idx)..end_idx.min(l.len())].to_vec();
                        Ok(Value::make_list(sliced))
                    }
                    Value::Str(s_val) => {
                        let chars: Vec<char> = s_val.chars().collect();
                        let len = chars.len() as i64;
                        let start_idx = if let Some(s_expr) = s {
                            let v = Box::pin(self.eval_expr(s_expr, env, ctx.clone())).await?;
                            let i = v.as_int().unwrap_or(0);
                            if i < 0 { (len + i).max(0) as usize } else { i.min(len) as usize }
                        } else { 0 };
                        let end_idx = if let Some(e_expr) = e {
                            let v = Box::pin(self.eval_expr(e_expr, env, ctx.clone())).await?;
                            let i = v.as_int().unwrap_or(len);
                            if i < 0 { (len + i).max(0) as usize } else { i.min(len) as usize }
                        } else { len as usize };
                        let sliced: String = chars[start_idx.min(end_idx)..end_idx.min(chars.len())].iter().collect();
                        Ok(Value::make_str(sliced))
                    }
                    _ => Err(runtime_err!("slice requires list or str")),
                }
            }

            Expr::StructLit { type_name, fields } => {
                // Treat struct literals as maps with __type__ for impl dispatch
                let mut m = HashMap::new();
                m.insert("__type__".to_string(), Value::make_str(type_name.as_str()));
                for (k, v_expr) in fields {
                    let val = Box::pin(self.eval_expr(v_expr, env, ctx.clone())).await?;
                    m.insert(k.clone(), val);
                }
                Ok(Value::make_map(m))
            }

            Expr::Parallel(exprs) => {
                // Sequential execution (true parallel would need owned Env per branch)
                let mut results = Vec::new();
                for e in exprs {
                    let v = Box::pin(self.eval_expr(e, env, ctx.clone())).await?;
                    results.push(v);
                }
                Ok(Value::make_list(results))
            }

            // Feature 3: optional chaining field access
            Expr::OptionalField { object, field } => {
                let obj = Box::pin(self.eval_expr(object, env, ctx.clone())).await?;
                match &obj {
                    Value::Null => Ok(Value::Null),
                    Value::Ctx(c)  => Ok(c.borrow().get_field(field)),
                    Value::Map(m)  => Ok(m.borrow().get(field.as_str()).cloned().unwrap_or(Value::Null)),
                    _ => Ok(Value::Null),
                }
            }

            // Feature 3: optional chaining method call
            Expr::OptionalMethod { object, method, args } => {
                let obj = Box::pin(self.eval_expr(object, env, ctx.clone())).await?;
                if matches!(obj, Value::Null) {
                    return Ok(Value::Null);
                }
                let mut evaluated_args = Vec::with_capacity(args.len());
                for a in args {
                    evaluated_args.push(Box::pin(self.eval_expr(a, env, ctx.clone())).await?);
                }
                self.call_method(obj, method, evaluated_args)
            }

            // Feature 2: list comprehension
            Expr::ListComp { expr: body_expr, var, iter, cond } => {
                let iterable = Box::pin(self.eval_expr(iter, env, ctx.clone())).await?;
                let items: Vec<Value> = match &iterable {
                    Value::List(l) => l.borrow().clone(),
                    _ => return Err(type_err!("iterable", iterable.type_name())),
                };
                let mut result = Vec::new();
                for item in items {
                    env.push();
                    env.define(var, item);
                    if let Some(cond_expr) = cond {
                        let cond_val = Box::pin(self.eval_expr(cond_expr, env, ctx.clone())).await?;
                        if !cond_val.is_truthy() {
                            env.pop();
                            continue;
                        }
                    }
                    let val = Box::pin(self.eval_expr(body_expr, env, ctx.clone())).await?;
                    result.push(val);
                    env.pop();
                }
                Ok(Value::make_list(result))
            }

            // Feature 6: `expr?` — try/unwrap Result
            Expr::Try(inner) => {
                let val = Box::pin(self.eval_expr(inner, env, ctx.clone())).await?;
                // If val is a Map with __result__ key, unwrap or propagate error
                if let Value::Map(ref m) = val {
                    let m_ref = m.borrow();
                    if let Some(Value::Str(result_type)) = m_ref.get("__result__") {
                        if result_type.as_str() == "err" {
                            let err = m_ref.get("error").cloned().unwrap_or(Value::Null);
                            // Create err result map and return as error
                            let mut err_map = HashMap::new();
                            err_map.insert("__result__".to_string(), Value::make_str("err"));
                            err_map.insert("error".to_string(), err);
                            return Err(GravError::Runtime(format!("try? propagated error: {}", m_ref.get("error").map(|v| v.to_string()).unwrap_or_default())));
                        } else if result_type.as_str() == "ok" {
                            return Ok(m_ref.get("value").cloned().unwrap_or(Value::Null));
                        }
                    }
                }
                // Not a Result — return as-is
                Ok(val)
            }

            Expr::Sandbox { config } => {
                // Feature 12: sandbox — isolated code execution
                let mut timeout_ms: u64 = 5000;
                let mut code_str = String::new();
                let mut denied: std::collections::HashSet<String> = std::collections::HashSet::new();
                for (key, val_expr) in config {
                    let val = Box::pin(self.eval_expr(val_expr, env, ctx.clone())).await?;
                    match key.as_str() {
                        "timeout" => timeout_ms = val.as_int().unwrap_or(5000) as u64,
                        "code"    => code_str = val.to_string(),
                        "deny"    => {
                            if let Value::List(l) = &val {
                                for item in l.borrow().iter() {
                                    denied.insert(item.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // Parse and execute code in a sandboxed interpreter
                let sandbox_interp = Interpreter::new(String::new(), String::new());
                {
                    let mut st = sandbox_interp.shared.lock().await;
                    st.denied_fns = denied;
                }
                let parse_result = (|| {
                    let tokens = crate::lexer::Lexer::new(&code_str).tokenize()?;
                    crate::parser::Parser::new(tokens).parse()
                })();
                match parse_result {
                    Ok(prog) => {
                        let stmts: Vec<crate::ast::Stmt> = prog.items.iter().filter_map(|i| {
                            if let crate::ast::Item::Stmt(s) = i { Some(s.clone()) } else { None }
                        }).collect();
                        // Use Box::pin for the async block to avoid infinite future sizing
                        let result = Box::pin(async {
                            let _ = sandbox_interp.load(&prog).await;
                            let mut sandbox_env = super::Env::new();
                            let mut sandbox_outputs = Vec::new();
                            sandbox_interp.exec_block_pub(&stmts, &mut sandbox_env, None, &mut sandbox_outputs).await
                        });
                        match tokio::time::timeout(
                            tokio::time::Duration::from_millis(timeout_ms),
                            result,
                        ).await {
                            Ok(Ok(v)) => Ok(v),
                            Ok(Err(e)) => Err(runtime_err!("sandbox error: {e}")),
                            Err(_) => Err(runtime_err!("sandbox timeout")),
                        }
                    }
                    Err(e) => Err(runtime_err!("sandbox parse error: {e}")),
                }
            }

            // Feature N3: with_breaker — circuit breaker pattern
            Expr::WithBreaker { name, body } => {
                // Check breaker state
                let (is_open, half_open) = {
                    let st = self.shared.lock().await;
                    if let Some(breaker) = st.breakers.get(name) {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        match breaker.status {
                            super::BreakerStatus::Open => {
                                if now >= breaker.last_failure + breaker.timeout_ms {
                                    (false, true)
                                } else {
                                    (true, false)
                                }
                            }
                            super::BreakerStatus::HalfOpen => (false, true),
                            super::BreakerStatus::Closed => (false, false),
                        }
                    } else {
                        (false, false)
                    }
                };
                if is_open {
                    return Err(runtime_err!("circuit breaker '{name}' is open"));
                }
                // Execute body
                let mut dummy_out = Vec::new();
                let result = Box::pin(self.exec_block(body, env, ctx.clone(), &mut dummy_out)).await;
                match result {
                    Ok(v) => {
                        // Success — reset failures if half-open, close breaker
                        if half_open {
                            let mut st = self.shared.lock().await;
                            if let Some(breaker) = st.breakers.get_mut(name) {
                                breaker.failure_count = 0;
                                breaker.status = super::BreakerStatus::Closed;
                            }
                        }
                        Ok(v)
                    }
                    Err(super::exec::ExecErr::Return(v)) => Ok(v),
                    Err(super::exec::ExecErr::Err(e)) => {
                        // Failure — increment count, maybe open
                        let mut st = self.shared.lock().await;
                        if let Some(breaker) = st.breakers.get_mut(name) {
                            breaker.failure_count += 1;
                            if breaker.failure_count >= breaker.threshold {
                                breaker.status = super::BreakerStatus::Open;
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64;
                                breaker.last_failure = now;
                            }
                        }
                        Err(e)
                    }
                    Err(_) => Ok(Value::Null),
                }
            }

            Expr::Cache { key, ttl_secs, body } => {
                let key_val = Box::pin(self.eval_expr(key, env, ctx.clone())).await?;
                let key_str = key_val.to_string();
                let ttl_val = Box::pin(self.eval_expr(ttl_secs, env, ctx.clone())).await?;
                let ttl_ms  = ttl_val.as_int().unwrap_or(300) as u64 * 1000;

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                // Check cache
                {
                    let st = self.shared.lock().await;
                    if let Some((cached_val, expires_at)) = st.cache_store.get(&key_str) {
                        if now_ms < *expires_at {
                            return Ok(cached_val.clone());
                        }
                    }
                }

                // Execute body
                let mut dummy_out = Vec::new();
                let result = match Box::pin(self.exec_block(body, env, ctx, &mut dummy_out)).await {
                    Ok(v) => v,
                    Err(super::exec::ExecErr::Return(v)) => v,
                    Err(super::exec::ExecErr::Err(e)) => return Err(e),
                    Err(_) => Value::Null,
                };

                // Store in cache
                {
                    let mut st = self.shared.lock().await;
                    st.cache_store.insert(key_str, (result.clone(), now_ms + ttl_ms));
                }

                Ok(result)
            }

            // Feature W1: form expression
            Expr::Form { fields, submit } => {
                let mut result_fields = Vec::new();
                for field in fields {
                    let kind_str = match &field.kind {
                        crate::ast::FormFieldKind::Text     => "text",
                        crate::ast::FormFieldKind::Textarea => "textarea",
                        crate::ast::FormFieldKind::Number   => "number",
                        crate::ast::FormFieldKind::Email    => "email",
                        crate::ast::FormFieldKind::Phone    => "phone",
                        crate::ast::FormFieldKind::Rating(_, _) => "rating",
                        crate::ast::FormFieldKind::Select(_)    => "select",
                    };
                    result_fields.push((field.name.clone(), kind_str.to_string()));
                }
                let submit_label = submit.clone().unwrap_or_else(|| "Submit".to_string());
                let mut map = HashMap::new();
                map.insert("__form__".to_string(), Value::Bool(true));
                let fields_list: Vec<Value> = result_fields.iter().map(|(name, kind)| {
                    let mut fm = HashMap::new();
                    fm.insert("name".to_string(), Value::make_str(name.as_str()));
                    fm.insert("kind".to_string(), Value::make_str(kind.as_str()));
                    Value::make_map(fm)
                }).collect();
                map.insert("fields".to_string(), Value::make_list(fields_list));
                map.insert("submit".to_string(), Value::make_str(submit_label));
                Ok(Value::make_map(map))
            }

            // Feature W4: websocket expression (stub)
            Expr::WebSocket { url, config } => {
                let url_val = Box::pin(self.eval_expr(url, env, ctx.clone())).await?;
                let mut ws_config = HashMap::new();
                ws_config.insert("url".to_string(), url_val.clone());
                for (key, val_expr) in config {
                    let val = Box::pin(self.eval_expr(val_expr, env, ctx.clone())).await?;
                    ws_config.insert(key.clone(), val);
                }
                // Return a stub map representing the websocket connection
                let mut map = HashMap::new();
                map.insert("__websocket__".to_string(), Value::Bool(true));
                map.insert("url".to_string(), url_val);
                map.insert("status".to_string(), Value::make_str("stub"));
                Ok(Value::make_map(map))
            }
        }
    }

    // ── evaluate interpolation hole by re-parsing ─────────────────────────────

    pub(crate) async fn eval_hole(&self, src: &str, env: &mut Env, ctx: Option<Rc<RefCell<BotCtx>>>) -> GravResult<Value> {
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        let tokens = Lexer::new(src).tokenize()?;
        let mut p = Parser::new(tokens);
        let prog = p.parse_expr_pub()?;
        Box::pin(self.eval_expr(&prog, env, ctx)).await
    }

    // ── function call ─────────────────────────────────────────────────────────

    pub(crate) async fn call_fn(
        &self,
        name: &str,
        args: Vec<Value>,
        _env: &mut Env,
        ctx:  Option<Rc<RefCell<BotCtx>>>,
    ) -> GravResult<Value> {
        // Feature 12: check denied_fns (sandbox)
        {
            let st = self.shared.lock().await;
            if st.denied_fns.contains(name) {
                return Err(runtime_err!("function '{name}' is denied in sandbox"));
            }
        }

        // Feature 5: check mocks first
        {
            let mock_body = {
                let st = self.shared.lock().await;
                st.mocks.get(name).cloned()
            };
            if let Some(body) = mock_body {
                let mut mock_env = Env::new();
                if let Some(c) = &ctx { mock_env.define("ctx", Value::Ctx(c.clone())); }
                for (i, arg) in args.iter().enumerate() {
                    mock_env.define(&format!("arg{i}"), arg.clone());
                }
                let mut dummy_out = Vec::new();
                return match Box::pin(self.exec_block(&body, &mut mock_env, ctx, &mut dummy_out)).await {
                    Ok(v) => Ok(v),
                    Err(ExecErr::Return(v)) => Ok(v),
                    Err(ExecErr::Err(e)) => Err(e),
                    Err(_) => Ok(Value::Null),
                };
            }
        }

        // stdlib first
        if let Some(v) = crate::stdlib::call_builtin(name, &args, &self.shared).await? {
            return Ok(v);
        }
        let fd = {
            let st = self.shared.lock().await;
            st.functions.get(name).cloned()
        };
        let fd = fd.ok_or_else(|| {
            // Collect function names for "did you mean?" suggestion
            let fn_names: Vec<String> = {
                // We can't await inside this closure, so use try_lock
                if let Ok(st) = self.shared.try_lock() {
                    st.functions.keys().cloned().collect()
                } else {
                    Vec::new()
                }
            };
            let suggestion = crate::error::did_you_mean(name, &fn_names)
                .map(|s| format!(" (did you mean `{}`?)", s))
                .unwrap_or_default();
            GravError::UndefinedFn(format!("{}{}", name, suggestion))
        })?;

        // Feature 1: handle decorators
        let decorators = fd.decorators.clone();
        let mut retry_count: u32 = 1;
        let mut is_logged = false;
        let mut cache_ttl: Option<u64> = None;

        for dec in &decorators {
            match dec.name.as_str() {
                "retry" => {
                    if let Some(n_expr) = dec.args.first() {
                        let n = Box::pin(self.eval_expr(n_expr, _env, ctx.clone())).await?
                            .as_int().unwrap_or(1) as u32;
                        retry_count = n.max(1);
                    }
                }
                "logged" => { is_logged = true; }
                "cached" => {
                    let ttl = if let Some(t_expr) = dec.args.first() {
                        Box::pin(self.eval_expr(t_expr, _env, ctx.clone())).await?
                            .as_int().unwrap_or(300) as u64
                    } else { 300 };
                    cache_ttl = Some(ttl);
                }
                _ => {}
            }
        }

        // Feature 1: @cached — check cache
        if let Some(ttl) = cache_ttl {
            let cache_key = format!("{}:{}", name, args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(","));
            let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
            let cached = {
                let st = self.shared.lock().await;
                st.cache_store.get(&cache_key).cloned()
            };
            if let Some((val, expires)) = cached {
                if now_ms < expires { return Ok(val); }
            }
            // Execute and cache result below
            let result = self.call_fn_inner(&fd, args, ctx.clone(), is_logged, retry_count).await?;
            {
                let mut st = self.shared.lock().await;
                st.cache_store.insert(cache_key, (result.clone(), now_ms + ttl * 1000));
            }
            return Ok(result);
        }

        self.call_fn_inner(&fd, args, ctx, is_logged, retry_count).await
    }

    async fn call_fn_inner(
        &self,
        fd:          &std::rc::Rc<crate::ast::FnDef>,
        args:        Vec<Value>,
        ctx:         Option<Rc<RefCell<BotCtx>>>,
        is_logged:   bool,
        retry_count: u32,
    ) -> GravResult<Value> {
        if args.len() != fd.params.len() {
            return Err(GravError::Arity { name: fd.name.to_string(), expected: fd.params.len(), got: args.len() });
        }
        if is_logged {
            eprintln!("[gravitix] CALL {}({})", fd.name, args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", "));
        }

        let mut last_err = None;
        for attempt in 0..retry_count {
            let mut fn_env = Env::new();
            if let Some(c) = &ctx { fn_env.define("ctx", Value::Ctx(c.clone())); }
            for (param, val) in fd.params.iter().zip(args.iter()) {
                fn_env.define(&param.name, val.clone());
            }
            let mut dummy_out = Vec::new();
            let result = match Box::pin(self.exec_block(&fd.body, &mut fn_env, ctx.clone(), &mut dummy_out)).await {
                Ok(v)                    => Ok(v),
                Err(ExecErr::Return(v))  => Ok(v),
                Err(ExecErr::Err(e))     => Err(e),
                Err(_)                   => Ok(Value::Null),
            };
            // Feature 4: execute deferred blocks in reverse order
            let defers = fn_env.take_defers();
            for defer_body in defers {
                let _ = Box::pin(self.exec_block(&defer_body, &mut fn_env, ctx.clone(), &mut dummy_out)).await;
            }
            match result {
                Ok(v) => {
                    if is_logged {
                        eprintln!("[gravitix] RETURN {}() = {v}", fd.name);
                    }
                    return Ok(v);
                }
                Err(e) => {
                    if attempt + 1 < retry_count {
                        eprintln!("[gravitix] @retry {}/{}: {}", attempt + 1, retry_count, e);
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| runtime_err!("retry exhausted")))
    }

    // ── method call on a value ────────────────────────────────────────────────

    pub(crate) fn call_method(&self, obj: Value, method: &str, args: Vec<Value>) -> GravResult<Value> {
        match &obj {
            Value::Str(s) => {
                match method {
                    "len"        => Ok(Value::Int(s.len() as i64)),
                    "to_upper"   => Ok(Value::make_str(s.to_uppercase())),
                    "to_lower"   => Ok(Value::make_str(s.to_lowercase())),
                    "trim"       => Ok(Value::make_str(s.trim().to_string())),
                    "starts_with"=> {
                        let p = args.first().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                        Ok(Value::Bool(s.starts_with(p.as_str())))
                    }
                    "ends_with"  => {
                        let p = args.first().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                        Ok(Value::Bool(s.ends_with(p.as_str())))
                    }
                    "contains"   => {
                        let p = args.first().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                        Ok(Value::Bool(s.contains(p.as_str())))
                    }
                    "split"      => {
                        let sep = args.first().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                        let parts: Vec<Value> = s.split(sep.as_str()).map(Value::make_str).collect();
                        Ok(Value::make_list(parts))
                    }
                    "replace"    => {
                        let from = args.get(0).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                        let to   = args.get(1).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                        Ok(Value::make_str(s.replace(from.as_str(), to.as_str())))
                    }
                    _ => Err(runtime_err!("str has no method '{method}'")),
                }
            }
            Value::List(l) => {
                match method {
                    "len"    => Ok(Value::Int(l.borrow().len() as i64)),
                    "push"   => { l.borrow_mut().extend(args); Ok(Value::Null) }
                    "pop"    => Ok(l.borrow_mut().pop().unwrap_or(Value::Null)),
                    "first"  => Ok(l.borrow().first().cloned().unwrap_or(Value::Null)),
                    "last"   => Ok(l.borrow().last().cloned().unwrap_or(Value::Null)),
                    "join"   => {
                        let sep = args.first().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                        let s: String = l.borrow().iter().map(|v| v.to_string()).collect::<Vec<_>>().join(&sep);
                        Ok(Value::make_str(s))
                    }
                    "contains" => {
                        let target = args.into_iter().next().unwrap_or(Value::Null);
                        Ok(Value::Bool(l.borrow().iter().any(|v| v == &target)))
                    }
                    // Feature 4: chain methods on lists
                    "map" => {
                        let fn_val = args.into_iter().next().unwrap_or(Value::Null);
                        let items = l.borrow().clone();
                        let result = self.list_chain_map(items, fn_val)?;
                        Ok(Value::make_list(result))
                    }
                    "filter" => {
                        let fn_val = args.into_iter().next().unwrap_or(Value::Null);
                        let items = l.borrow().clone();
                        let result = self.list_chain_filter(items, fn_val)?;
                        Ok(Value::make_list(result))
                    }
                    "reduce" => {
                        let fn_val = args.into_iter().next().unwrap_or(Value::Null);
                        let items = l.borrow().clone();
                        self.list_chain_reduce(items, fn_val)
                    }
                    "find" => {
                        let fn_val = args.into_iter().next().unwrap_or(Value::Null);
                        let items = l.borrow().clone();
                        self.list_chain_find(items, fn_val)
                    }
                    "sort" => {
                        let fn_val = args.into_iter().next();
                        let mut items = l.borrow().clone();
                        if fn_val.is_none() || matches!(fn_val, Some(Value::Null)) {
                            // Natural sort
                            items.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        }
                        // Sort with comparator function would need async — use natural sort
                        Ok(Value::make_list(items))
                    }
                    "flat_map" => {
                        let fn_val = args.into_iter().next().unwrap_or(Value::Null);
                        let items = l.borrow().clone();
                        let mapped = self.list_chain_map(items, fn_val)?;
                        let mut result = Vec::new();
                        for v in mapped {
                            if let Value::List(inner) = v {
                                result.extend(inner.borrow().clone());
                            } else {
                                result.push(v);
                            }
                        }
                        Ok(Value::make_list(result))
                    }
                    "any" => {
                        let fn_val = args.into_iter().next().unwrap_or(Value::Null);
                        let items = l.borrow().clone();
                        for item in &items {
                            let res = self.call_lambda_sync(&fn_val, vec![item.clone()])?;
                            if res.is_truthy() { return Ok(Value::Bool(true)); }
                        }
                        Ok(Value::Bool(false))
                    }
                    "all" => {
                        let fn_val = args.into_iter().next().unwrap_or(Value::Null);
                        let items = l.borrow().clone();
                        for item in &items {
                            let res = self.call_lambda_sync(&fn_val, vec![item.clone()])?;
                            if !res.is_truthy() { return Ok(Value::Bool(false)); }
                        }
                        Ok(Value::Bool(true))
                    }
                    "enumerate" => {
                        let items = l.borrow().clone();
                        let result: Vec<Value> = items.into_iter().enumerate()
                            .map(|(i, v)| Value::make_list(vec![Value::Int(i as i64), v]))
                            .collect();
                        Ok(Value::make_list(result))
                    }
                    _ => Err(runtime_err!("list has no method '{method}'")),
                }
            }
            Value::Map(m) => {
                // Feature 5: expect methods
                {
                    let m_ref = m.borrow();
                    if let Some(expect_val) = m_ref.get("__expect__").cloned() {
                        drop(m_ref);
                        match method {
                            "to_equal" => {
                                let expected = args.into_iter().next().unwrap_or(Value::Null);
                                if expect_val != expected {
                                    return Err(runtime_err!("expect: {} != {}", expect_val, expected));
                                }
                                return Ok(Value::Null);
                            }
                            "to_not_be_null" => {
                                if matches!(expect_val, Value::Null) {
                                    return Err(runtime_err!("expect: value is null"));
                                }
                                return Ok(Value::Null);
                            }
                            "to_contain" => {
                                let substr = args.into_iter().next().unwrap_or(Value::Null).to_string();
                                if !expect_val.to_string().contains(&substr) {
                                    return Err(runtime_err!("expect: '{}' does not contain '{}'", expect_val, substr));
                                }
                                return Ok(Value::Null);
                            }
                            "to_throw" => {
                                // expect_val should be a lambda; call it, expect error
                                if let Value::Fn(fd) = &expect_val {
                                    let mut fn_env = super::Env::new();
                                    for (param, val) in fd.params.iter().zip(args) {
                                        fn_env.define(&param.name, val);
                                    }
                                    let result = self.call_lambda_sync(&expect_val, vec![]);
                                    match result {
                                        Err(_) => return Ok(Value::Null), // expected throw
                                        Ok(_) => return Err(runtime_err!("expect: expected function to throw")),
                                    }
                                }
                                return Err(runtime_err!("expect: to_throw requires a function"));
                            }
                            _ => {}
                        }
                    }
                }
                // Feature 2: impl method dispatch
                {
                    let m_ref = m.borrow();
                    if let Some(Value::Str(type_name)) = m_ref.get("__type__") {
                        let type_name = type_name.as_ref().clone();
                        drop(m_ref);
                        // Check for impl method — need to do this sync so we return a special marker
                        // Actually we can call it directly since call_method is sync
                        // We need the shared state, but it requires async. Use try_lock.
                        if let Ok(st) = self.shared.try_lock() {
                            if let Some(fd) = st.impl_methods.get(&(type_name, method.to_string())).cloned() {
                                drop(st);
                                // Call the method: bind self + args
                                let mut fn_env = super::Env::new();
                                fn_env.define("self", obj.clone());
                                // Skip "self" parameter (first param)
                                let param_start = if fd.params.first().map_or(false, |p| p.name == "self") { 1 } else { 0 };
                                for (param, val) in fd.params.iter().skip(param_start).zip(args) {
                                    fn_env.define(&param.name, val);
                                }
                                // We need async here — return a placeholder. Actually call_method is sync.
                                // We'll return a marker that can be handled. For simplicity, let's
                                // use a different approach: store the result in an immediate eval.
                                // Since we can't await here, we'll return a special __impl_call__ marker.
                                let mut call_map = HashMap::new();
                                call_map.insert("__impl_call__".to_string(), Value::Fn(fd));
                                call_map.insert("__self__".to_string(), obj.clone());
                                return Ok(Value::make_map(call_map));
                            }
                        }
                    }
                }
                // Feature 11: db query builder
                {
                    let m_ref = m.borrow();
                    if m_ref.get("__dbquery__").is_some() {
                        drop(m_ref);
                        return self.db_query_chain(obj, method, args);
                    }
                }
                match method {
                    "len"    => Ok(Value::Int(m.borrow().len() as i64)),
                    "keys"   => Ok(Value::make_list(m.borrow().keys().map(Value::make_str).collect())),
                    "values" => Ok(Value::make_list(m.borrow().values().cloned().collect())),
                    "has"    => {
                        let k = args.first().map(|v| v.to_string()).unwrap_or_default();
                        Ok(Value::Bool(m.borrow().contains_key(&k)))
                    }
                    "remove" => {
                        let k = args.first().map(|v| v.to_string()).unwrap_or_default();
                        Ok(m.borrow_mut().remove(&k).unwrap_or(Value::Null))
                    }
                    _ => Err(runtime_err!("map has no method '{method}'")),
                }
            }
            _ => Err(runtime_err!("{} has no method '{method}'", obj.type_name())),
        }
    }

    // ── Feature 4: list chain method helpers ─────────────────────────────────

    fn call_lambda_sync(&self, fn_val: &Value, args: Vec<Value>) -> GravResult<Value> {
        if let Value::Fn(fd) = fn_val {
            let mut fn_env = super::Env::new();
            for (param, val) in fd.params.iter().zip(args) {
                fn_env.define(&param.name, val);
            }
            // Execute body synchronously — we run it in a simple loop
            // For sync context, return the last expression value
            // This is a simplified sync execution for lambdas in chain methods
            let mut last = Value::Null;
            for stmt in &fd.body {
                match stmt {
                    crate::ast::Stmt::Return(Some(e)) => {
                        return self.eval_expr_sync(e, &mut fn_env);
                    }
                    crate::ast::Stmt::Expr(e) => {
                        last = self.eval_expr_sync(e, &mut fn_env)?;
                    }
                    _ => {}
                }
            }
            Ok(last)
        } else {
            Ok(Value::Null)
        }
    }

    fn eval_expr_sync(&self, expr: &crate::ast::Expr, env: &mut super::Env) -> GravResult<Value> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Null => Ok(Value::Null),
            Expr::Var(name) => env.get(name).ok_or_else(|| runtime_err!("undefined: {name}")),
            Expr::Binary { op, lhs, rhs } => {
                let l = self.eval_expr_sync(lhs, env)?;
                let r = self.eval_expr_sync(rhs, env)?;
                crate::value::apply_binop(op.clone(), l, r)
            }
            Expr::Unary { op, expr: inner } => {
                let v = self.eval_expr_sync(inner, env)?;
                match op {
                    UnaryOp::Neg => match v {
                        Value::Int(n) => Ok(Value::Int(-n)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        Value::Complex(r, i) => Ok(Value::Complex(-r, -i)),
                        _ => Err(type_err!("number", v.type_name())),
                    },
                    UnaryOp::Not => Ok(Value::Bool(!v.is_truthy())),
                    UnaryOp::BitNot => match v {
                        Value::Int(n) => Ok(Value::Int(!n)),
                        _ => Err(runtime_err!("bitwise NOT (~) requires integer")),
                    },
                }
            }
            Expr::Field { object, field } => {
                let obj = self.eval_expr_sync(object, env)?;
                match &obj {
                    Value::Map(m) => Ok(m.borrow().get(field.as_str()).cloned().unwrap_or(Value::Null)),
                    _ => Err(runtime_err!("cannot access field '{}' on {}", field, obj.type_name())),
                }
            }
            Expr::Method { object, method, args } => {
                let obj = self.eval_expr_sync(object, env)?;
                let mut evals = Vec::new();
                for a in args {
                    evals.push(self.eval_expr_sync(a, env)?);
                }
                self.call_method(obj, method, evals)
            }
            Expr::Call { name: _, args: call_args } => {
                let mut evals = Vec::new();
                for a in call_args {
                    evals.push(self.eval_expr_sync(a, env)?);
                }
                // Only support basic operations in sync context
                Ok(Value::Null)
            }
            Expr::Str(parts) => {
                let mut out = String::new();
                for part in parts {
                    match part {
                        crate::lexer::StrPart::Lit(s) => out.push_str(s),
                        crate::lexer::StrPart::Hole(src) => {
                            // Simple: try to look up as variable
                            if let Some(v) = env.get(src) {
                                out.push_str(&v.to_string());
                            }
                        }
                    }
                }
                Ok(Value::make_str(out))
            }
            _ => Ok(Value::Null),
        }
    }

    fn list_chain_map(&self, items: Vec<Value>, fn_val: Value) -> GravResult<Vec<Value>> {
        let mut result = Vec::new();
        for item in items {
            let v = self.call_lambda_sync(&fn_val, vec![item])?;
            result.push(v);
        }
        Ok(result)
    }

    fn list_chain_filter(&self, items: Vec<Value>, fn_val: Value) -> GravResult<Vec<Value>> {
        let mut result = Vec::new();
        for item in items {
            let v = self.call_lambda_sync(&fn_val, vec![item.clone()])?;
            if v.is_truthy() { result.push(item); }
        }
        Ok(result)
    }

    fn list_chain_reduce(&self, items: Vec<Value>, fn_val: Value) -> GravResult<Value> {
        let mut iter = items.into_iter();
        let mut acc = match iter.next() {
            Some(v) => v,
            None => return Ok(Value::Null),
        };
        for item in iter {
            acc = self.call_lambda_sync(&fn_val, vec![acc, item])?;
        }
        Ok(acc)
    }

    fn list_chain_find(&self, items: Vec<Value>, fn_val: Value) -> GravResult<Value> {
        for item in items {
            let v = self.call_lambda_sync(&fn_val, vec![item.clone()])?;
            if v.is_truthy() { return Ok(item); }
        }
        Ok(Value::Null)
    }

    // ── Feature 11: db query builder ─────────────────────────────────────────

    fn db_query_chain(&self, obj: Value, method: &str, args: Vec<Value>) -> GravResult<Value> {
        if let Value::Map(m) = &obj {
            let mut new_map = m.borrow().clone();
            match method {
                "where" => {
                    if let Some(filter) = args.into_iter().next() {
                        new_map.insert("filters".to_string(), filter);
                    }
                    Ok(Value::make_map(new_map))
                }
                "sort" => {
                    let field = args.get(0).map(|v| v.to_string()).unwrap_or_default();
                    let dir   = args.get(1).map(|v| v.to_string()).unwrap_or_else(|| "asc".to_string());
                    new_map.insert("sort_field".to_string(), Value::make_str(field));
                    new_map.insert("sort_dir".to_string(), Value::make_str(dir));
                    Ok(Value::make_map(new_map))
                }
                "limit" => {
                    let n = args.first().and_then(|v| v.as_int()).unwrap_or(10);
                    new_map.insert("limit_n".to_string(), Value::Int(n));
                    Ok(Value::make_map(new_map))
                }
                "exec" => {
                    // Execute query against DB
                    let collection = new_map.get("collection").map(|v| v.to_string()).unwrap_or_default();
                    let limit_n = new_map.get("limit_n").and_then(|v| v.as_int()).unwrap_or(1000) as usize;
                    let sort_field = new_map.get("sort_field").map(|v| v.to_string());
                    let sort_dir = new_map.get("sort_dir").map(|v| v.to_string()).unwrap_or_else(|| "asc".to_string());
                    let filters = new_map.get("filters").cloned();

                    let st = self.shared.try_lock()
                        .map_err(|_| runtime_err!("db query: could not lock shared state"))?;
                    let all_rows = st.db.all(&collection);
                    drop(st);

                    let mut results: Vec<Value> = all_rows.into_iter()
                        .map(|(k, v)| {
                            let mut row = HashMap::new();
                            row.insert("key".to_string(), Value::make_str(k));
                            match &v {
                                Value::Map(inner) => {
                                    for (ik, iv) in inner.borrow().iter() {
                                        row.insert(ik.clone(), iv.clone());
                                    }
                                }
                                _ => { row.insert("value".to_string(), v); }
                            }
                            Value::make_map(row)
                        })
                        .collect();

                    // Apply filters
                    if let Some(Value::Map(filter_map)) = &filters {
                        let fm = filter_map.borrow();
                        results.retain(|row| {
                            if let Value::Map(row_map) = row {
                                let rm = row_map.borrow();
                                for (field, condition) in fm.iter() {
                                    let row_val = rm.get(field).cloned().unwrap_or(Value::Null);
                                    if let Value::Map(cond_map) = condition {
                                        let cm = cond_map.borrow();
                                        for (op, expected) in cm.iter() {
                                            let pass = match op.as_str() {
                                                "gt"  => row_val > *expected,
                                                "lt"  => row_val < *expected,
                                                "gte" => row_val >= *expected,
                                                "lte" => row_val <= *expected,
                                                "eq"  => row_val == *expected,
                                                "ne"  => row_val != *expected,
                                                "contains" => {
                                                    row_val.as_str().map_or(false, |s| {
                                                        s.contains(&expected.to_string())
                                                    })
                                                }
                                                _ => true,
                                            };
                                            if !pass { return false; }
                                        }
                                    } else {
                                        // Direct equality check
                                        if row_val != *condition { return false; }
                                    }
                                }
                                true
                            } else {
                                true
                            }
                        });
                    }

                    // Sort
                    if let Some(ref sf) = sort_field {
                        let sf = sf.clone();
                        let ascending = sort_dir != "desc";
                        results.sort_by(|a, b| {
                            let va = if let Value::Map(m) = a { m.borrow().get(&sf).cloned().unwrap_or(Value::Null) } else { Value::Null };
                            let vb = if let Value::Map(m) = b { m.borrow().get(&sf).cloned().unwrap_or(Value::Null) } else { Value::Null };
                            let ord = va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal);
                            if ascending { ord } else { ord.reverse() }
                        });
                    }

                    // Limit
                    results.truncate(limit_n);
                    Ok(Value::make_list(results))
                }
                _ => Err(runtime_err!("db query has no method '{method}'")),
            }
        } else {
            Err(runtime_err!("db query chain on non-map"))
        }
    }
}
