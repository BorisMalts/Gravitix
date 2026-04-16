use std::rc::Rc;
use std::cell::RefCell;

#[allow(unused_imports)]
use crate::ast::*;
use crate::error::GravError;
use crate::value::{BotCtx, BotOutput, Value};
use crate::{runtime_err, type_err};
use super::{Interpreter, Env};
#[allow(unused_imports)]
use crate::ast::Pattern;

// ─────────────────────────────────────────────────────────────────────────────
// Internal control-flow error type (not public)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) enum ExecErr {
    Err(GravError),
    Return(Value),
    Break,
    Continue,
    /// Stop — used by hook middleware to abort handler chain
    Stop,
}

impl From<GravError> for ExecErr {
    fn from(e: GravError) -> Self { ExecErr::Err(e) }
}

impl std::fmt::Display for ExecErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecErr::Err(e)      => write!(f, "{e}"),
            ExecErr::Return(v)   => write!(f, "return {v}"),
            ExecErr::Break       => write!(f, "break"),
            ExecErr::Continue    => write!(f, "continue"),
            ExecErr::Stop        => write!(f, "stop"),
        }
    }
}

impl Interpreter {
    // ── execute a block of statements ─────────────────────────────────────────

    pub(crate) async fn exec_block(
        &self,
        stmts:   &[Stmt],
        env:     &mut Env,
        ctx:     Option<Rc<RefCell<BotCtx>>>,
        outputs: &mut Vec<BotOutput>,
    ) -> Result<Value, ExecErr> {
        env.push();
        let mut last = Value::Null;
        for stmt in stmts {
            last = Box::pin(self.exec_stmt(stmt, env, ctx.clone(), outputs)).await?;
        }
        env.pop();
        Ok(last)
    }

    // ── execute a statement ───────────────────────────────────────────────────

    pub(crate) async fn exec_stmt(
        &self,
        stmt:    &Stmt,
        env:     &mut Env,
        ctx:     Option<Rc<RefCell<BotCtx>>>,
        outputs: &mut Vec<BotOutput>,
    ) -> Result<Value, ExecErr> {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let v = self.eval_expr(value, env, ctx.clone()).await?;
                env.define(name, v);
                Ok(Value::Null)
            }

            Stmt::Assign { target, value } => {
                let v = self.eval_expr(value, env, ctx.clone()).await?;
                self.assign_target(target, v, env, ctx.clone()).await?;
                Ok(Value::Null)
            }

            Stmt::CompoundAssign { target, op, value } => {
                let current = self.eval_expr(target, env, ctx.clone()).await?;
                let rhs     = self.eval_expr(value, env, ctx.clone()).await?;
                let result  = crate::value::apply_binop(op.clone(), current, rhs).map_err(ExecErr::Err)?;
                self.assign_target(target, result, env, ctx.clone()).await?;
                Ok(Value::Null)
            }

            Stmt::Emit(expr) => {
                let v = self.eval_expr(expr, env, ctx.clone()).await?;
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                outputs.push(BotOutput::Send { room_id, text: v.to_string() });
                Ok(Value::Null)
            }

            Stmt::EmitBroadcast(expr) => {
                let text = self.eval_expr(expr, env, ctx.clone()).await?.to_string();
                let rooms = {
                    let st = self.shared.lock().await;
                    st.known_rooms.clone()
                };
                for room_id in rooms {
                    outputs.push(BotOutput::Send { room_id, text: text.clone() });
                }
                Ok(Value::Null)
            }

            Stmt::EmitTo { target, msg } => {
                let room_id_val = self.eval_expr(target, env, ctx.clone()).await?;
                let text_val    = self.eval_expr(msg, env, ctx.clone()).await?;
                let room_id = room_id_val.as_int().unwrap_or(0);
                outputs.push(BotOutput::Send { room_id, text: text_val.to_string() });
                Ok(Value::Null)
            }

            Stmt::Reply { reply_to, text } => {
                let room_id  = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let reply_id = self.eval_expr(reply_to, env, ctx.clone()).await?
                    .as_int().unwrap_or(0);
                let text_val = self.eval_expr(text, env, ctx.clone()).await?;
                outputs.push(BotOutput::Reply { room_id, reply_to: reply_id, text: text_val.to_string() });
                Ok(Value::Null)
            }

            Stmt::DeleteMsg(expr) => {
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let msg_id  = self.eval_expr(expr, env, ctx.clone()).await?.as_int().unwrap_or(0);
                outputs.push(BotOutput::DeleteMessage { room_id, msg_id });
                Ok(Value::Null)
            }

            Stmt::Return(expr) => {
                let v = match expr {
                    Some(e) => self.eval_expr(e, env, ctx.clone()).await?,
                    None    => Value::Null,
                };
                Err(ExecErr::Return(v))
            }

            Stmt::Break    => Err(ExecErr::Break),
            Stmt::Continue => Err(ExecErr::Continue),

            Stmt::If { cond, then, elif, else_ } => {
                let c = self.eval_expr(cond, env, ctx.clone()).await?;
                if c.is_truthy() {
                    Box::pin(self.exec_block(then, env, ctx.clone(), outputs)).await?;
                } else {
                    let mut matched = false;
                    for (ec, eb) in elif {
                        let ev = self.eval_expr(ec, env, ctx.clone()).await?;
                        if ev.is_truthy() {
                            Box::pin(self.exec_block(eb, env, ctx.clone(), outputs)).await?;
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        if let Some(eb) = else_ {
                            Box::pin(self.exec_block(eb, env, ctx.clone(), outputs)).await?;
                        }
                    }
                }
                Ok(Value::Null)
            }

            Stmt::While { cond, body } => {
                loop {
                    let c = self.eval_expr(cond, env, ctx.clone()).await?;
                    if !c.is_truthy() { break; }
                    match Box::pin(self.exec_block(body, env, ctx.clone(), outputs)).await {
                        Ok(_) => {}
                        Err(ExecErr::Break) => break,
                        Err(ExecErr::Continue) => continue,
                        Err(e) => return Err(e),
                    }
                }
                Ok(Value::Null)
            }

            Stmt::For { var, iter, body } => {
                let iterable = self.eval_expr(iter, env, ctx.clone()).await?;
                let items: Vec<Value> = match &iterable {
                    Value::List(l) => l.borrow().clone(),
                    Value::Map(m)  => m.borrow().keys().map(|k| Value::make_str(k)).collect(),
                    _ => return Err(ExecErr::Err(type_err!("iterable", iterable.type_name()))),
                };
                for item in items {
                    env.push();
                    env.define(var, item);
                    match Box::pin(self.exec_block(body, env, ctx.clone(), outputs)).await {
                        Ok(_) => {}
                        Err(ExecErr::Break) => { env.pop(); break; }
                        Err(ExecErr::Continue) => { env.pop(); continue; }
                        Err(e) => { env.pop(); return Err(e); }
                    }
                    env.pop();
                }
                Ok(Value::Null)
            }

            Stmt::Match { subject, arms } => {
                let val = self.eval_expr(subject, env, ctx.clone()).await?;
                for arm in arms {
                    let caps = self.match_pattern(&arm.pattern, &val).await
                        .map_err(ExecErr::Err)?;
                    if let Some(captures) = caps {
                        // Bind capture groups into env
                        env.push();
                        for (i, cap) in captures.iter().enumerate() {
                            env.define(&format!("${i}"), Value::make_str(cap.clone()));
                        }
                        // Feature 1/6: bind enum destruct / Result pattern bindings
                        if let Pattern::EnumDestruct { enum_name, variant, bindings } = &arm.pattern {
                            if let Value::Map(m) = &val {
                                let m_ref = m.borrow();
                                if enum_name == "Result" {
                                    let key = match variant.as_str() {
                                        "Ok"  => "value",
                                        "Err" => "error",
                                        _     => "",
                                    };
                                    if !key.is_empty() {
                                        if let Some(first_binding) = bindings.first() {
                                            let v = m_ref.get(key).cloned().unwrap_or(Value::Null);
                                            env.define(first_binding, v);
                                        }
                                    }
                                } else {
                                    // Enum tuple variant bindings: __0__, __1__, ...
                                    for (i, binding) in bindings.iter().enumerate() {
                                        let v = m_ref.get(&format!("__{i}__")).cloned().unwrap_or(Value::Null);
                                        env.define(binding, v);
                                    }
                                }
                            }
                        }
                        let result = Box::pin(self.exec_block(&arm.body, env, ctx.clone(), outputs)).await;
                        env.pop();
                        result?;
                        break;
                    }
                }
                Ok(Value::Null)
            }

            Stmt::RunFlow(name) => {
                let ctx_rc = ctx.ok_or_else(|| ExecErr::Err(runtime_err!("run flow requires ctx")))?;
                self.run_flow(name, ctx_rc, outputs).await.map_err(ExecErr::Err)?;
                Ok(Value::Null)
            }

            Stmt::TryCatch { try_body, err_name, catch_body, finally_body } => {
                let result = match Box::pin(self.exec_block(try_body, env, ctx.clone(), outputs)).await {
                    Ok(v) => Ok(v),
                    Err(ExecErr::Err(e)) => {
                        env.push();
                        env.define(err_name, Value::make_str(e.to_string()));
                        let result = Box::pin(self.exec_block(catch_body, env, ctx.clone(), outputs)).await;
                        env.pop();
                        result
                    }
                    Err(other) => Err(other),
                };
                // Execute finally block regardless of outcome
                if !finally_body.is_empty() {
                    let _ = Box::pin(self.exec_block(finally_body, env, ctx.clone(), outputs)).await;
                }
                result
            }

            Stmt::SendKeyboard { text, buttons } => {
                let room_id  = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let text_val = self.eval_expr(text, env, ctx.clone()).await?;
                let btns_val = self.eval_expr(buttons, env, ctx.clone()).await?;
                let buttons = extract_keyboard_buttons(btns_val)
                    .map_err(ExecErr::Err)?;
                outputs.push(BotOutput::Keyboard { room_id, text: text_val.to_string(), buttons });
                Ok(Value::Null)
            }

            Stmt::EditMsg { msg_id, text } => {
                // EditMsg is not in BotOutput for Vortex — silently ignore or log
                let _ = self.eval_expr(msg_id, env, ctx.clone()).await?;
                let _ = self.eval_expr(text, env, ctx.clone()).await?;
                eprintln!("[gravitix] EditMsg not supported in Vortex backend (skipped)");
                Ok(Value::Null)
            }

            Stmt::AnswerCallback(text_expr) => {
                let cb_id = ctx.as_ref()
                    .and_then(|c| c.borrow().callback_id.clone())
                    .unwrap_or_default();
                let text = if let Some(expr) = text_expr {
                    Some(self.eval_expr(expr, env, ctx.clone()).await?.to_string())
                } else {
                    None
                };
                outputs.push(BotOutput::AnswerCallback { callback_id: cb_id, text });
                Ok(Value::Null)
            }

            Stmt::Stop => Err(ExecErr::Stop),

            Stmt::FederatedEmit { target, msg } => {
                let target_val = self.eval_expr(target, env, ctx.clone()).await?;
                let msg_val    = self.eval_expr(msg, env, ctx.clone()).await?;
                outputs.push(crate::value::BotOutput::FederatedSend {
                    target: target_val.to_string(),
                    text:   msg_val.to_string(),
                });
                Ok(Value::Null)
            }

            Stmt::AbTest(ab) => {
                let user_id = ctx.as_ref().map(|c| c.borrow().user_id).unwrap_or(0);
                let (variant_a_count, variant_b_count) = {
                    let st = self.shared.lock().await;
                    st.ab_results.get(&ab.name).copied().unwrap_or((0, 0))
                };
                let _ = (variant_a_count, variant_b_count);
                if user_id % 2 == 0 {
                    self.shared.lock().await.ab_results.entry(ab.name.clone())
                        .and_modify(|(a, _)| *a += 1)
                        .or_insert((1, 0));
                    Box::pin(self.exec_block(&ab.variant_a, env, ctx.clone(), outputs)).await?;
                } else {
                    self.shared.lock().await.ab_results.entry(ab.name.clone())
                        .and_modify(|(_, b)| *b += 1)
                        .or_insert((0, 1));
                    Box::pin(self.exec_block(&ab.variant_b, env, ctx.clone(), outputs)).await?;
                }
                Ok(Value::Null)
            }

            Stmt::Expr(expr) => {
                let val = self.eval_expr(expr, env, ctx.clone()).await.map_err(ExecErr::Err)?;
                // Feature 9: intercept chat action markers from typing/pin_msg/etc.
                if let Value::Map(ref m) = val {
                    let m_ref = m.borrow();
                    if let Some(Value::Str(action)) = m_ref.get("__action__") {
                        let room_id = m_ref.get("room_id").and_then(|v| v.as_int()).unwrap_or(0);
                        match action.as_str() {
                            "typing" => {
                                outputs.push(BotOutput::Typing { room_id });
                            }
                            "pin_msg" => {
                                let msg_id = m_ref.get("arg0").and_then(|v| v.as_int()).unwrap_or(0);
                                outputs.push(BotOutput::PinMsg { room_id, msg_id });
                            }
                            "unpin_msg" => {
                                let msg_id = m_ref.get("arg0").and_then(|v| v.as_int()).unwrap_or(0);
                                outputs.push(BotOutput::UnpinMsg { room_id, msg_id });
                            }
                            "mute_user" => {
                                let user_id = m_ref.get("arg0").and_then(|v| v.as_int()).unwrap_or(0);
                                let duration_ms = m_ref.get("arg1").and_then(|v| v.as_int()).map(|v| v as u64);
                                outputs.push(BotOutput::MuteUser { room_id, user_id, duration_ms });
                            }
                            "notify" => {
                                let user_id = m_ref.get("arg0").and_then(|v| v.as_int()).unwrap_or(0);
                                let text = m_ref.get("arg1").map(|v| v.to_string()).unwrap_or_default();
                                outputs.push(BotOutput::Notify { user_id, text });
                            }
                            "notify_room" => {
                                let nr_room_id = m_ref.get("arg0").and_then(|v| v.as_int()).unwrap_or(0);
                                let text = m_ref.get("arg1").map(|v| v.to_string()).unwrap_or_default();
                                outputs.push(BotOutput::NotifyRoom { room_id: nr_room_id, text });
                            }
                            _ => {}
                        }
                        return Ok(Value::Null);
                    }
                }
                Ok(val)
            }

            Stmt::Transition(state_name) => {
                // Store the FSM transition target in env for dispatch to pick up
                env.define("__fsm_transition__", Value::make_str(state_name.clone()));
                Ok(Value::Null)
            }

            Stmt::Assert { cond, msg } => {
                let v = self.eval_expr(cond, env, ctx.clone()).await?;
                if !v.is_truthy() {
                    let msg_str = if let Some(msg_expr) = msg {
                        self.eval_expr(msg_expr, env, ctx.clone()).await
                            .map(|v| v.to_string())
                            .unwrap_or_else(|_| "Assertion failed".into())
                    } else {
                        format!("Assertion failed: {:?}", cond)
                    };
                    return Err(ExecErr::Err(runtime_err!("{}", msg_str)));
                }
                Ok(Value::Null)
            }

            Stmt::EmitRich { fields } => {
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let mut title: Option<String>  = None;
                let mut text:  Option<String>  = None;
                let mut image: Option<String>  = None;
                let mut buttons: Vec<Vec<(String, String)>> = Vec::new();
                for (key, val_expr) in fields {
                    let val = self.eval_expr(val_expr, env, ctx.clone()).await?;
                    match key.as_str() {
                        "title"   => title   = Some(val.to_string()),
                        "text"    => text    = Some(val.to_string()),
                        "image"   => image   = Some(val.to_string()),
                        "buttons" => {
                            // buttons: [["Label", "data"], ...]
                            if let Value::List(rows) = &val {
                                for row_val in rows.borrow().iter() {
                                    if let Value::List(row) = row_val {
                                        let mut btn_row = Vec::new();
                                        for btn_val in row.borrow().iter() {
                                            if let Value::List(btn) = btn_val {
                                                let b = btn.borrow();
                                                let label = b.first().map(|v| v.to_string()).unwrap_or_default();
                                                let data  = b.get(1).map(|v| v.to_string()).unwrap_or_default();
                                                btn_row.push((label, data));
                                            }
                                        }
                                        buttons.push(btn_row);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                outputs.push(crate::value::BotOutput::RichMessage { room_id, title, text, image, buttons });
                Ok(Value::Null)
            }

            Stmt::RunFsm(name) => {
                let (user_id, room_id) = ctx.as_ref()
                    .map(|c| { let c = c.borrow(); (c.user_id, c.room_id) })
                    .unwrap_or((0, 0));
                let fsm = {
                    let st = self.shared.lock().await;
                    st.fsm_defs.get(name).cloned()
                };
                if let Some(fsm) = fsm {
                    let initial = fsm.initial.clone();
                    // Set FSM state for this user
                    self.shared.lock().await.fsm_states.insert((user_id, name.clone()), initial.clone());
                    // Run on_enter for initial state
                    if let Some(state) = fsm.states.iter().find(|s| s.name == initial) {
                        if !state.on_enter.is_empty() {
                            let mut env2 = Env::new();
                            if let Some(c) = &ctx { env2.define("ctx", Value::Ctx(c.clone())); }
                            Box::pin(self.exec_block(&state.on_enter, &mut env2, ctx.clone(), outputs)).await?;
                        }
                    }
                    // Suppress unused variable warning
                    let _ = room_id;
                } else {
                    return Err(ExecErr::Err(runtime_err!("undefined fsm '{name}'")));
                }
                Ok(Value::Null)
            }

            // Feature 1: map destructuring
            Stmt::LetDestructMap { fields, value } => {
                let v = self.eval_expr(value, env, ctx.clone()).await?;
                match &v {
                    Value::Map(m) => {
                        let m = m.borrow();
                        for f in fields {
                            let val = m.get(f.as_str()).cloned().unwrap_or(Value::Null);
                            env.define(f, val);
                        }
                    }
                    _ => {
                        // Try treating any value with field access
                        for f in fields {
                            env.define(f, Value::Null);
                        }
                    }
                }
                Ok(Value::Null)
            }

            // Feature 1: list destructuring
            Stmt::LetDestructList { items, rest, value } => {
                let v = self.eval_expr(value, env, ctx.clone()).await?;
                match &v {
                    Value::List(l) => {
                        let l = l.borrow();
                        for (i, name) in items.iter().enumerate() {
                            let val = l.get(i).cloned().unwrap_or(Value::Null);
                            env.define(name, val);
                        }
                        if let Some(rest_name) = rest {
                            let remaining = if items.len() < l.len() {
                                l[items.len()..].to_vec()
                            } else {
                                vec![]
                            };
                            env.define(rest_name, Value::make_list(remaining));
                        }
                    }
                    _ => {
                        for name in items { env.define(name, Value::Null); }
                        if let Some(rest_name) = rest {
                            env.define(rest_name, Value::make_list(vec![]));
                        }
                    }
                }
                Ok(Value::Null)
            }

            // Feature 4: defer
            Stmt::Defer { body } => {
                env.push_defer(body.clone());
                Ok(Value::Null)
            }

            // Feature 11: paginate
            Stmt::Paginate { items, page_size, format_fn, title } => {
                let items_val = self.eval_expr(items, env, ctx.clone()).await?;
                let page_size_val = self.eval_expr(page_size, env, ctx.clone()).await?
                    .as_int().unwrap_or(5) as usize;
                let page_size_val = page_size_val.max(1);
                let title_str = if let Some(t) = title {
                    self.eval_expr(t, env, ctx.clone()).await?.to_string()
                } else {
                    String::new()
                };

                let all_items: Vec<Value> = match &items_val {
                    Value::List(l) => l.borrow().clone(),
                    _ => vec![items_val],
                };

                let total_pages = (all_items.len() + page_size_val - 1) / page_size_val;
                let page_items = all_items.iter().take(page_size_val).cloned().collect::<Vec<_>>();
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);

                // Format items
                let mut formatted = Vec::new();
                for item in &page_items {
                    if let Some(fmt_fn_expr) = format_fn {
                        if let Value::Fn(fd) = self.eval_expr(fmt_fn_expr, env, ctx.clone()).await? {
                            let mut fn_env = super::Env::new();
                            if fd.params.len() == 1 {
                                fn_env.define(&fd.params[0].name, item.clone());
                            }
                            let mut dummy_out = Vec::new();
                            match Box::pin(self.exec_block(&fd.body, &mut fn_env, ctx.clone(), &mut dummy_out)).await {
                                Ok(v) => formatted.push(v.to_string()),
                                Err(super::exec::ExecErr::Return(v)) => formatted.push(v.to_string()),
                                _ => formatted.push(item.to_string()),
                            }
                        } else {
                            formatted.push(item.to_string());
                        }
                    } else {
                        formatted.push(item.to_string());
                    }
                }

                let text = if title_str.is_empty() {
                    format!("{}\n\nPage 1/{}", formatted.join("\n"), total_pages)
                } else {
                    format!("{}\n{}\n\nPage 1/{}", title_str, formatted.join("\n"), total_pages)
                };

                let mut buttons = Vec::new();
                let mut nav_row = Vec::new();
                nav_row.push(("< Prev".to_string(), "page_prev".to_string()));
                nav_row.push((format!("1/{total_pages}"), "_".to_string()));
                nav_row.push(("Next >".to_string(), "page_next".to_string()));
                buttons.push(nav_row);

                outputs.push(BotOutput::Keyboard { room_id, text, buttons });

                // Store pagination state
                let user_id = ctx.as_ref().map(|c| c.borrow().user_id).unwrap_or(0);
                {
                    let mut st = self.shared.lock().await;
                    st.paginations.insert((room_id, user_id), super::PaginationState {
                        items: all_items,
                        page: 0,
                        page_size: page_size_val,
                        title: title_str,
                    });
                }

                Ok(Value::Null)
            }

            // Feature 3: spawn { body }
            Stmt::Spawn { body } => {
                // Clone body and run in a separate local task
                let body_clone = body.clone();
                let interp_shared = self.shared.clone();
                let ctx_clone = ctx.clone();
                let _body = body_clone;
                let _shared = interp_shared;
                let _ctx = ctx_clone;
                // For now, execute inline (true spawn would need Arc<Interpreter>)
                // We execute the body in a fresh env
                let mut spawn_env = Env::new();
                if let Some(c) = &ctx {
                    spawn_env.define("ctx", Value::Ctx(c.clone()));
                }
                let mut spawn_outputs = Vec::new();
                let _ = Box::pin(self.exec_block(body, &mut spawn_env, ctx.clone(), &mut spawn_outputs)).await;
                outputs.extend(spawn_outputs);
                Ok(Value::Null)
            }

            // Feature 7: embed { key: val, ... }
            Stmt::Embed { fields } => {
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let mut html: Option<String> = None;
                let mut url: Option<String> = None;
                let mut height: i64 = 400;
                let mut title = String::new();
                for (key, val_expr) in fields {
                    let val = self.eval_expr(val_expr, env, ctx.clone()).await?;
                    match key.as_str() {
                        "html"   => html = Some(val.to_string()),
                        "url"    => url = Some(val.to_string()),
                        "height" => height = val.as_int().unwrap_or(400),
                        "title"  => title = val.to_string(),
                        _ => {}
                    }
                }
                outputs.push(BotOutput::Embed { room_id, html, url, height, title });
                Ok(Value::Null)
            }

            // Feature 8: enqueue "queue_name" { body }
            Stmt::Enqueue { queue_name, body } => {
                // Push body to queue's pending list
                let mut st = self.shared.lock().await;
                if let Some(q) = st.queues.get_mut(queue_name) {
                    q.pending.push_back(body.clone());
                } else {
                    return Err(ExecErr::Err(runtime_err!("undefined queue '{}'", queue_name)));
                }
                Ok(Value::Null)
            }

            // Feature 2: fire event
            Stmt::Fire { event, data } => {
                let event_name = self.eval_expr(event, env, ctx.clone()).await
                    .map_err(ExecErr::Err)?.to_string();
                let data_val = self.eval_expr(data, env, ctx.clone()).await
                    .map_err(ExecErr::Err)?;
                // Look up event handlers
                let handlers = {
                    let st = self.shared.lock().await;
                    st.event_handlers.get(&event_name).cloned().unwrap_or_default()
                };
                for handler_body in &handlers {
                    let mut ev_env = Env::new();
                    if let Some(c) = &ctx { ev_env.define("ctx", Value::Ctx(c.clone())); }
                    ev_env.define("event_data", data_val.clone());
                    let _ = Box::pin(self.exec_block(handler_body, &mut ev_env, ctx.clone(), outputs)).await;
                }
                Ok(Value::Null)
            }

            // Feature 4: select (simplified — execute first matching arm)
            Stmt::Select { arms } => {
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let user_id = ctx.as_ref().map(|c| c.borrow().user_id).unwrap_or(0);

                // Find the timeout arm (if any)
                let timeout_ms = arms.iter().find_map(|arm| {
                    if let crate::ast::SelectKind::Timeout(ms) = &arm.kind { Some(*ms) } else { None }
                }).unwrap_or(300_000);

                // Set up a combined wait — first matching update wins
                let (tx, rx) = tokio::sync::oneshot::channel::<String>();
                let has_msg = arms.iter().any(|a| matches!(a.kind, crate::ast::SelectKind::WaitMsg));
                let has_cb  = arms.iter().any(|a| matches!(a.kind, crate::ast::SelectKind::WaitCallback(_)));

                if has_msg {
                    self.shared.lock().await.wait_map.insert((room_id, user_id), tx);
                } else if has_cb {
                    self.shared.lock().await.callback_wait_map.insert((room_id, user_id), tx);
                } else {
                    // Only timeout arm — just wait
                    tokio::time::sleep(tokio::time::Duration::from_millis(timeout_ms)).await;
                    // Execute timeout arm
                    for arm in arms {
                        if matches!(arm.kind, crate::ast::SelectKind::Timeout(_)) {
                            Box::pin(self.exec_block(&arm.body, env, ctx.clone(), outputs)).await?;
                            return Ok(Value::Null);
                        }
                    }
                    return Ok(Value::Null);
                }

                match tokio::time::timeout(tokio::time::Duration::from_millis(timeout_ms), rx).await {
                    Ok(Ok(data)) => {
                        // Find matching arm
                        for arm in arms {
                            let matches = match &arm.kind {
                                crate::ast::SelectKind::WaitMsg => has_msg,
                                crate::ast::SelectKind::WaitCallback(prefix) => {
                                    prefix.as_ref().map_or(true, |p| data.starts_with(p.as_str()))
                                }
                                crate::ast::SelectKind::Timeout(_) => false,
                            };
                            if matches {
                                env.push();
                                env.define("msg", Value::make_str(data.clone()));
                                let result = Box::pin(self.exec_block(&arm.body, env, ctx.clone(), outputs)).await;
                                env.pop();
                                result?;
                                break;
                            }
                        }
                    }
                    Ok(Err(_)) | Err(_) => {
                        // Timeout — execute timeout arm
                        for arm in arms {
                            if matches!(arm.kind, crate::ast::SelectKind::Timeout(_)) {
                                Box::pin(self.exec_block(&arm.body, env, ctx.clone(), outputs)).await?;
                                break;
                            }
                        }
                    }
                }
                Ok(Value::Null)
            }

            // Feature 5: mock
            Stmt::Mock { target, body } => {
                let mut st = self.shared.lock().await;
                st.mocks.insert(target.clone(), body.clone());
                Ok(Value::Null)
            }

            // Feature 6: validate
            Stmt::Validate { value, kind, or_body } => {
                let val = self.eval_expr(value, env, ctx.clone()).await
                    .map_err(ExecErr::Err)?;
                let val_str = val.to_string();
                let valid = match kind.name.as_str() {
                    "email" => {
                        let re = regex::Regex::new(r"^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$").unwrap();
                        re.is_match(&val_str)
                    }
                    "phone" => {
                        let re = regex::Regex::new(r"^\+?[0-9]{7,15}$").unwrap();
                        re.is_match(&val_str)
                    }
                    "url" => val_str.starts_with("http://") || val_str.starts_with("https://"),
                    "int" => {
                        if let Ok(n) = val_str.parse::<i64>() {
                            if kind.args.len() == 2 {
                                let min = self.eval_expr(&kind.args[0], env, ctx.clone()).await
                                    .map_err(ExecErr::Err)?.as_int().unwrap_or(i64::MIN);
                                let max = self.eval_expr(&kind.args[1], env, ctx.clone()).await
                                    .map_err(ExecErr::Err)?.as_int().unwrap_or(i64::MAX);
                                n >= min && n <= max
                            } else { true }
                        } else { false }
                    }
                    "float" => val_str.parse::<f64>().is_ok(),
                    "len" => {
                        let len = val_str.len();
                        if kind.args.len() == 2 {
                            let min = self.eval_expr(&kind.args[0], env, ctx.clone()).await
                                .map_err(ExecErr::Err)?.as_int().unwrap_or(0) as usize;
                            let max = self.eval_expr(&kind.args[1], env, ctx.clone()).await
                                .map_err(ExecErr::Err)?.as_int().unwrap_or(usize::MAX as i64) as usize;
                            len >= min && len <= max
                        } else { true }
                    }
                    _ => true, // unknown validation kind passes
                };
                if !valid {
                    Box::pin(self.exec_block(or_body, env, ctx.clone(), outputs)).await?;
                }
                Ok(Value::Null)
            }

            // Feature 8: batch
            Stmt::Batch { body } => {
                let mut batch_outputs = Vec::new();
                Box::pin(self.exec_block(body, env, ctx.clone(), &mut batch_outputs)).await?;
                // Merge consecutive Send outputs to same room
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let mut merged_text = String::new();
                for out in &batch_outputs {
                    match out {
                        BotOutput::Send { text, .. } => {
                            if !merged_text.is_empty() { merged_text.push('\n'); }
                            merged_text.push_str(text);
                        }
                        other => {
                            if !merged_text.is_empty() {
                                outputs.push(BotOutput::Send { room_id, text: std::mem::take(&mut merged_text) });
                            }
                            outputs.push(other.clone());
                        }
                    }
                }
                if !merged_text.is_empty() {
                    outputs.push(BotOutput::Send { room_id, text: merged_text });
                }
                Ok(Value::Null)
            }

            // Feature 11: use middleware (no-op at exec level; handled in dispatch)
            Stmt::UseMiddleware(name) => {
                let mut st = self.shared.lock().await;
                st.middleware_chain.push(name.clone());
                Ok(Value::Null)
            }

            // Feature N8: breakpoint — print all vars to stderr
            Stmt::Breakpoint => {
                eprintln!("[gravitix] BREAKPOINT hit");
                for (name, val) in env.all_vars() {
                    eprintln!("  {name} = {val}");
                }
                Ok(Value::Null)
            }

            // Feature N8: debug block — execute and print each expr to stderr
            Stmt::Debug { body } => {
                eprintln!("[gravitix] DEBUG {{");
                for s in body {
                    let v = Box::pin(self.exec_stmt(s, env, ctx.clone(), outputs)).await?;
                    eprintln!("  => {v}");
                }
                eprintln!("[gravitix] }}");
                Ok(Value::Null)
            }

            // Feature N12: simulate — handled in run_scenario_test, no-op in normal exec
            Stmt::Simulate { .. } => Ok(Value::Null),

            // Feature N12: expect_reply — handled in run_scenario_test, no-op in normal exec
            Stmt::ExpectReply { .. } => Ok(Value::Null),

            // Feature W2: table { config }
            Stmt::Table { config } => {
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let mut columns: Vec<String> = Vec::new();
                let mut rows_val = Value::Null;
                let mut page_size: usize = 10;
                for (key, val_expr) in config {
                    let val = self.eval_expr(val_expr, env, ctx.clone()).await?;
                    match key.as_str() {
                        "columns" => {
                            if let Value::List(l) = &val {
                                columns = l.borrow().iter().map(|v| v.to_string()).collect();
                            }
                        }
                        "rows" | "data" => rows_val = val,
                        "page_size" => page_size = val.as_int().unwrap_or(10) as usize,
                        _ => {}
                    }
                }
                // Build ASCII table
                let rows: Vec<Vec<String>> = match &rows_val {
                    Value::List(l) => l.borrow().iter().map(|row| {
                        match row {
                            Value::List(cols) => cols.borrow().iter().map(|v| v.to_string()).collect(),
                            Value::Map(m) => columns.iter().map(|c| m.borrow().get(c).map(|v| v.to_string()).unwrap_or_default()).collect(),
                            _ => vec![row.to_string()],
                        }
                    }).collect(),
                    _ => vec![],
                };
                let _ = page_size;
                let header = columns.join(" | ");
                let sep = columns.iter().map(|c| "-".repeat(c.len().max(5))).collect::<Vec<_>>().join("-+-");
                let body_lines: Vec<String> = rows.iter().map(|r| r.join(" | ")).collect();
                let text = format!("{header}\n{sep}\n{}", body_lines.join("\n"));
                outputs.push(BotOutput::Table { room_id, text });
                Ok(Value::Null)
            }

            // Feature W3: chart { config }
            Stmt::Chart { config } => {
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let mut chart_type = "bar".to_string();
                let mut data_val = Value::Null;
                let mut title = String::new();
                for (key, val_expr) in config {
                    let val = self.eval_expr(val_expr, env, ctx.clone()).await?;
                    match key.as_str() {
                        "type" | "kind" => chart_type = val.to_string(),
                        "data" => data_val = val,
                        "title" => title = val.to_string(),
                        _ => {}
                    }
                }
                // Build ASCII chart
                let entries: Vec<(String, f64)> = match &data_val {
                    Value::Map(m) => m.borrow().iter().map(|(k, v)| (k.clone(), v.as_float().unwrap_or(0.0))).collect(),
                    Value::List(l) => l.borrow().iter().enumerate().map(|(i, v)| (format!("{i}"), v.as_float().unwrap_or(0.0))).collect(),
                    _ => vec![],
                };
                let max_val = entries.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).max(1.0);
                let _ = chart_type;
                let mut text = if !title.is_empty() { format!("{title}\n") } else { String::new() };
                for (label, val) in &entries {
                    let bar_len = ((val / max_val) * 30.0) as usize;
                    let bar = "#".repeat(bar_len);
                    text.push_str(&format!("{:>10} | {bar} {val}\n", label));
                }
                outputs.push(BotOutput::Send { room_id, text });
                Ok(Value::Null)
            }

            // Feature W6: stream { body }
            Stmt::Stream { body } => {
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                // Execute body; each emit becomes a StreamChunk
                let mut stream_outputs = Vec::new();
                let _ = Box::pin(self.exec_block(body, env, ctx.clone(), &mut stream_outputs)).await;
                for out in stream_outputs {
                    match out {
                        BotOutput::Send { text, .. } => {
                            outputs.push(BotOutput::StreamChunk { room_id, text });
                        }
                        other => outputs.push(other),
                    }
                }
                Ok(Value::Null)
            }

            Stmt::Wizard { output_var, steps } => {
                let room_id = ctx.as_ref().map(|c| c.borrow().room_id).unwrap_or(0);
                let user_id = ctx.as_ref().map(|c| c.borrow().user_id).unwrap_or(0);
                let mut result = std::collections::HashMap::new();

                for step in steps {
                    let prompt_text = self.eval_expr(&step.prompt, env, ctx.clone()).await
                        .map_err(ExecErr::Err)?.to_string();

                    if step.is_confirm {
                        // Send keyboard with Yes/No
                        let buttons = vec![vec![
                            ("Yes".to_string(), "wizard_yes".to_string()),
                            ("No".to_string(),  "wizard_no".to_string()),
                        ]];
                        outputs.push(BotOutput::Keyboard { room_id, text: prompt_text, buttons });

                        // Wait for callback
                        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
                        self.shared.lock().await.callback_wait_map.insert((room_id, user_id), tx);
                        let cb = match tokio::time::timeout(tokio::time::Duration::from_secs(120), rx).await {
                            Ok(Ok(d)) => d,
                            _ => {
                                outputs.push(BotOutput::Send { room_id, text: "Wizard timed out.".into() });
                                return Ok(Value::Null);
                            }
                        };
                        if cb != "wizard_yes" {
                            outputs.push(BotOutput::Send { room_id, text: "Cancelled.".into() });
                            return Ok(Value::Null);
                        }
                        continue;
                    }

                    // Regular ask step
                    outputs.push(BotOutput::Send { room_id, text: prompt_text });

                    // Wait for message
                    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
                    self.shared.lock().await.wait_map.insert((room_id, user_id), tx);
                    let response = match tokio::time::timeout(tokio::time::Duration::from_secs(120), rx).await {
                        Ok(Ok(text)) => text,
                        _ => {
                            outputs.push(BotOutput::Send { room_id, text: "Wizard timed out.".into() });
                            return Ok(Value::Null);
                        }
                    };

                    // Convert to type
                    let value = convert_wizard_value(response, &step.ty)
                        .map_err(|e| ExecErr::Err(runtime_err!("{e}")))?;

                    // Validate
                    if let Some(validate_lambda) = &step.validate {
                        env.push();
                        env.define(&step.var, value.clone());
                        let valid = Box::pin(self.eval_expr(validate_lambda, env, ctx.clone())).await
                            .map_err(ExecErr::Err)?;
                        env.pop();
                        if !valid.is_truthy() {
                            outputs.push(BotOutput::Send { room_id, text: "Invalid value, wizard cancelled.".into() });
                            return Ok(Value::Null);
                        }
                    }

                    // Also define in current env so later steps can reference it
                    env.define(&step.var, value.clone());
                    result.insert(step.var.clone(), value);
                }

                env.define(output_var, Value::make_map(result));
                Ok(Value::Null)
            }
        }
    }
}

fn convert_wizard_value(s: String, ty: &crate::ast::TypeExpr) -> Result<Value, String> {
    match ty {
        crate::ast::TypeExpr::Int | crate::ast::TypeExpr::Float => {
            if let Ok(i) = s.trim().parse::<i64>() {
                Ok(Value::Int(i))
            } else if let Ok(f) = s.trim().parse::<f64>() {
                Ok(Value::Float(f))
            } else {
                Err(format!("Expected number, got '{s}'"))
            }
        }
        crate::ast::TypeExpr::Bool => {
            match s.trim().to_lowercase().as_str() {
                "yes" | "true" | "1" => Ok(Value::Bool(true)),
                _ => Ok(Value::Bool(false)),
            }
        }
        _ => Ok(Value::make_str(s)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: parse keyboard buttons from a Value
// Expected format: list<list<list<str>>>
//   e.g. [[ ["OK", "ok_data"], ["Cancel", "cancel_data"] ]]
// ─────────────────────────────────────────────────────────────────────────────

fn extract_keyboard_buttons(val: Value) -> crate::error::GravResult<Vec<Vec<(String, String)>>> {
    let rows = match val {
        Value::List(l) => l.borrow().clone(),
        _ => return Err(crate::runtime_err!("send_keyboard: buttons must be a list")),
    };
    let mut result = Vec::new();
    for row_val in rows {
        let row_list = match row_val {
            Value::List(l) => l.borrow().clone(),
            _ => return Err(crate::runtime_err!("send_keyboard: each row must be a list")),
        };
        let mut row = Vec::new();
        for btn_val in row_list {
            let btn_list = match btn_val {
                Value::List(l) => l.borrow().clone(),
                _ => return Err(crate::runtime_err!("send_keyboard: each button must be [label, data]")),
            };
            if btn_list.len() < 2 {
                return Err(crate::runtime_err!("send_keyboard: button needs [label, callback_data]"));
            }
            let label = btn_list[0].to_string();
            let data  = btn_list[1].to_string();
            row.push((label, data));
        }
        result.push(row);
    }
    Ok(result)
}
