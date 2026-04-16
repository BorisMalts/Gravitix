use std::rc::Rc;
use std::cell::RefCell;

use crate::ast::*;
use crate::error::GravResult;
use crate::value::{BotCtx, BotOutput, Value, UpdateKind};
use crate::runtime_err;
use super::{Interpreter, Env};
use super::exec::ExecErr;

impl Interpreter {
    pub async fn dispatch(
        &self,
        prog:        &Program,
        ctx:         BotCtx,
        update_type: &str,
    ) -> GravResult<Vec<BotOutput>> {
        let room_id      = ctx.room_id;
        let user_id      = ctx.user_id;
        let text         = ctx.text.clone().unwrap_or_default();
        let cmd          = ctx.command.clone().unwrap_or_default();
        let cb_data      = ctx.callback_data.clone().unwrap_or_default();
        let reaction     = ctx.reaction.clone();
        let update_kind  = ctx.update_kind.clone();

        // Track known rooms (capped at 10_000)
        {
            let mut st = self.shared.lock().await;
            if !st.known_rooms.contains(&room_id) {
                if st.known_rooms.len() >= 10_000 { st.known_rooms.remove(0); }
                st.known_rooms.push(room_id);
            }

            // Feature 8: update last_activity and last_room per user
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            st.last_activity.insert(user_id, now_ms);
            st.last_room.insert(user_id, room_id);

            // Feature 10: push message to history (keep max 50 per user/room pair)
            if !text.is_empty() {
                let entry = st.message_history.entry((room_id, user_id)).or_default();
                let mut msg_map = std::collections::HashMap::new();
                msg_map.insert("text".to_string(), Value::make_str(text.clone()));
                msg_map.insert("user_id".to_string(), Value::Int(user_id));
                msg_map.insert("user_name".to_string(), Value::make_str(ctx.username.clone()));
                msg_map.insert("timestamp".to_string(), Value::Int(ctx.timestamp));
                entry.push_back(Value::make_map(msg_map));
                while entry.len() > 50 { entry.pop_front(); }
            }

            // Check callback wait_map for "wait callback" flows
            if update_type == "callback" {
                if let Some(sender) = st.callback_wait_map.remove(&(room_id, user_id)) {
                    let _ = sender.send(cb_data.clone());
                    return Ok(vec![]);
                }
            }

            // Check message wait_map for "wait msg" flows
            if update_type == "message" {
                if let Some(sender) = st.wait_map.remove(&(room_id, user_id)) {
                    let _ = sender.send(text.clone());
                    return Ok(vec![]);
                }
            }
        }

        let ctx_rc = Rc::new(RefCell::new(ctx));
        let mut outputs: Vec<BotOutput> = Vec::new();

        // Feature 3: run before-hooks
        let before_hooks = {
            let st = self.shared.lock().await;
            st.before_hooks.clone()
        };
        let mut hook_stopped = false;
        for hook_body in &before_hooks {
            let mut env = Env::new();
            env.define("ctx", Value::Ctx(ctx_rc.clone()));
            match self.exec_block(hook_body, &mut env, Some(ctx_rc.clone()), &mut outputs).await {
                Ok(_) | Err(ExecErr::Return(_)) => {}
                Err(ExecErr::Stop) => { hook_stopped = true; break; }
                Err(ExecErr::Err(e)) => return Err(e),
                Err(ExecErr::Break | ExecErr::Continue) => {}
            }
        }
        if hook_stopped { return Ok(outputs); }

        // ── FSM routing ───────────────────────────────────────────────────────
        // Check if any FSM is active for this user and handle it first
        let active_fsm = {
            let st = self.shared.lock().await;
            // Find any active FSM for this user
            st.fsm_states.iter()
                .find(|((uid, _), _)| *uid == user_id)
                .map(|((_, fsm_name), state_name)| (fsm_name.clone(), state_name.clone()))
        };

        if let Some((fsm_name, current_state)) = active_fsm {
            let fsm_def = {
                let st = self.shared.lock().await;
                st.fsm_defs.get(&fsm_name).cloned()
            };
            if let Some(fsm) = fsm_def {
                if let Some(state) = fsm.states.iter().find(|s| s.name == current_state) {
                    for fh in &state.handlers {
                        let matches = match &fh.trigger {
                            crate::ast::FsmTrigger::Command(c) => update_type == "command" && cmd == *c,
                            crate::ast::FsmTrigger::AnyMsg => update_type == "message",
                            crate::ast::FsmTrigger::Other(t) => update_type == t.as_str(),
                        };
                        if !matches { continue; }

                        let mut env = Env::new();
                        env.define("ctx", Value::Ctx(ctx_rc.clone()));
                        match self.exec_block(&fh.body, &mut env, Some(ctx_rc.clone()), &mut outputs).await {
                            Ok(_) | Err(ExecErr::Return(_)) => {}
                            Err(ExecErr::Err(e)) => return Err(e),
                            Err(ExecErr::Break | ExecErr::Continue | ExecErr::Stop) => {}
                        }

                        // Check if a transition was requested
                        if let Some(Value::Str(next_state)) = env.get("__fsm_transition__") {
                            let next_state = next_state.as_ref().clone();
                            // Run on_leave for current state
                            if !state.on_leave.is_empty() {
                                let mut env2 = Env::new();
                                env2.define("ctx", Value::Ctx(ctx_rc.clone()));
                                let _ = self.exec_block(&state.on_leave, &mut env2, Some(ctx_rc.clone()), &mut outputs).await;
                            }
                            // Update FSM state
                            self.shared.lock().await.fsm_states.insert((user_id, fsm_name.clone()), next_state.clone());
                            // Run on_enter for next state
                            if let Some(next_st) = fsm.states.iter().find(|s| s.name == next_state) {
                                if !next_st.on_enter.is_empty() {
                                    let mut env2 = Env::new();
                                    env2.define("ctx", Value::Ctx(ctx_rc.clone()));
                                    let _ = self.exec_block(&next_st.on_enter, &mut env2, Some(ctx_rc.clone()), &mut outputs).await;
                                }
                            }
                        }
                        return Ok(outputs);
                    }
                }
            }
        }

        // ── Feature N5: Canary routing ─────────────────────────────────────────
        let canaries = {
            let st = self.shared.lock().await;
            st.canaries.clone()
        };
        let mut canary_handled = false;
        for canary in &canaries {
            if (user_id % 100) < canary.percent as i64 {
                for handler in &canary.handlers {
                    let matches = match &handler.trigger {
                        Trigger::Command(cmd_name) => update_type == "command" && cmd == *cmd_name,
                        Trigger::AnyMsg => update_type == "message",
                        _ => false,
                    };
                    if matches {
                        let mut env = Env::new();
                        env.define("ctx", Value::Ctx(ctx_rc.clone()));
                        let _ = self.exec_block(&handler.body, &mut env, Some(ctx_rc.clone()), &mut outputs).await;
                        canary_handled = true;
                        break;
                    }
                }
                if canary_handled { break; }
            }
        }
        if canary_handled {
            // Run after hooks and return
            let after_hooks = {
                let st = self.shared.lock().await;
                st.after_hooks.clone()
            };
            for hook_body in &after_hooks {
                let mut env = Env::new();
                env.define("ctx", Value::Ctx(ctx_rc.clone()));
                let _ = self.exec_block(hook_body, &mut env, Some(ctx_rc.clone()), &mut outputs).await;
            }
            return Ok(outputs);
        }

        // ── Feature N1: Intent matching (after Commands, before AnyMsg) ─────
        let intent_defs = {
            let st = self.shared.lock().await;
            st.intent_defs.clone()
        };
        let mut matched_intent: Option<String> = None;
        if !intent_defs.is_empty() && update_type == "message" && !text.is_empty() {
            let text_lower = text.to_lowercase();
            for (intent_name, phrases) in &intent_defs {
                for phrase in phrases {
                    let phrase_lower = phrase.to_lowercase();
                    if text_lower.contains(&phrase_lower) {
                        matched_intent = Some(intent_name.clone());
                        break;
                    }
                    // Simple Levenshtein for short phrases (<=6 chars)
                    if phrase_lower.len() <= 6 {
                        for word in text_lower.split_whitespace() {
                            if levenshtein_distance(word, &phrase_lower) <= 2 {
                                matched_intent = Some(intent_name.clone());
                                break;
                            }
                        }
                    }
                    if matched_intent.is_some() { break; }
                }
                if matched_intent.is_some() { break; }
            }
            // Set intent on ctx
            if let Some(ref intent) = matched_intent {
                ctx_rc.borrow_mut().intent = Some(intent.clone());
            }
        }

        // ── Regular handler dispatch ──────────────────────────────────────────
        for (handler_idx, item) in prog.items.iter().enumerate() {
            let Item::Handler(handler) = item else { continue };
            let matches = match &handler.trigger {
                Trigger::Command(cmd_name) => {
                    update_type == "command" && cmd == *cmd_name
                }
                Trigger::AnyMsg => update_type == "message",
                Trigger::Callback(prefix) => {
                    update_type == "callback" && match prefix {
                        None    => true,
                        Some(p) => cb_data.starts_with(p.as_str()),
                    }
                }
                Trigger::Join     => update_type == "join",
                Trigger::Leave    => update_type == "leave",
                Trigger::EditedMsg => update_type == "edited",
                Trigger::Any      => true,
                Trigger::Error    => false, // error handlers are invoked separately
                Trigger::Reaction(emoji) => {
                    update_type == "reaction" && match emoji {
                        None    => true,
                        Some(e) => reaction.as_deref() == Some(e.as_str()),
                    }
                }
                // Feature 2: Media triggers
                Trigger::File     => update_kind == UpdateKind::File,
                Trigger::Image    => update_kind == UpdateKind::Image,
                Trigger::VoiceMsg => update_kind == UpdateKind::VoiceMsg,
                // Feature 4: Mention / DM triggers
                Trigger::Mention  => update_kind == UpdateKind::Mention,
                Trigger::Dm       => update_kind == UpdateKind::Dm || ctx_rc.borrow().is_dm,
                // Feature 8: Idle — only fires from idle checker, not here
                Trigger::Idle(_)  => false,
                // Feature 10: Webhook — matched by webhook path
                Trigger::Webhook(_path) => update_type == "webhook",
                // Feature 10: new triggers
                Trigger::PollVote => update_kind == UpdateKind::PollVote,
                Trigger::Thread   => update_kind == UpdateKind::Thread,
                Trigger::Forward  => update_kind == UpdateKind::Forward,
                // Event triggers are handled by fire statement, not dispatch
                Trigger::Event(_) => false,
                // Feature N1: intent triggers
                Trigger::Intent(intent_name) => {
                    matched_intent.as_deref() == Some(intent_name.as_str())
                }
                Trigger::IntentUnknown => {
                    update_type == "message" && !intent_defs.is_empty() && matched_intent.is_none()
                }
            };
            if !matches { continue; }

            if let Some(guard_expr) = &handler.guard {
                let mut env = Env::new();
                env.define("ctx", Value::Ctx(ctx_rc.clone()));
                let guard_val = self.eval_expr(guard_expr, &mut env, Some(ctx_rc.clone())).await?;
                if !guard_val.is_truthy() { continue; }
            }

            // Permission check
            if let Some(perm_name) = &handler.require {
                let perm_expr = {
                    let st = self.shared.lock().await;
                    st.permissions.get(perm_name).cloned()
                };
                if let Some(perm_expr) = perm_expr {
                    let mut env = Env::new();
                    env.define("ctx", Value::Ctx(ctx_rc.clone()));
                    let perm_val = self.eval_expr(&perm_expr, &mut env, Some(ctx_rc.clone())).await?;
                    if !perm_val.is_truthy() { continue; }
                } else {
                    // Unknown permission — deny
                    continue;
                }
            }

            // Rate limit check
            if let Some(rl) = &handler.ratelimit {
                let allowed = {
                    let mut st = self.shared.lock().await;
                    st.check_ratelimit(rl, room_id, user_id, handler_idx)
                };
                if !allowed {
                    if let Some(msg) = &rl.cooldown {
                        outputs.push(BotOutput::Send { room_id, text: msg.clone() });
                    }
                    continue;
                }
            }

            let mut env = Env::new();
            env.define("ctx", Value::Ctx(ctx_rc.clone()));
            let exec_result = self.exec_block(&handler.body, &mut env, Some(ctx_rc.clone()), &mut outputs).await;
            match exec_result {
                Ok(_) => {}
                Err(ExecErr::Return(_)) => {}
                Err(ExecErr::Stop | ExecErr::Break | ExecErr::Continue) => {}
                Err(ExecErr::Err(e)) => {
                    // Try to find an error handler
                    let error_handler = prog.items.iter().find(|item| {
                        matches!(item, Item::Handler(h) if matches!(h.trigger, Trigger::Error))
                    });
                    if let Some(Item::Handler(eh)) = error_handler {
                        let err_msg = e.to_string();
                        let mut err_map = std::collections::HashMap::new();
                        err_map.insert("message".to_string(), Value::make_str(err_msg));
                        let mut env2 = Env::new();
                        env2.define("ctx", Value::Ctx(ctx_rc.clone()));
                        env2.define("error", Value::make_map(err_map));
                        let _ = self.exec_block(&eh.body, &mut env2, Some(ctx_rc.clone()), &mut outputs).await;
                    } else {
                        return Err(e);
                    }
                }
            }
            break;
        }

        // Feature 3: run after-hooks
        let after_hooks = {
            let st = self.shared.lock().await;
            st.after_hooks.clone()
        };
        for hook_body in &after_hooks {
            let mut env = Env::new();
            env.define("ctx", Value::Ctx(ctx_rc.clone()));
            match self.exec_block(hook_body, &mut env, Some(ctx_rc.clone()), &mut outputs).await {
                Ok(_) | Err(ExecErr::Return(_)) | Err(ExecErr::Stop) => {}
                Err(ExecErr::Err(e)) => return Err(e),
                Err(ExecErr::Break | ExecErr::Continue) => {}
            }
        }

        Ok(outputs)
    }

    pub async fn run_flow(
        &self,
        name:    &str,
        ctx_rc:  Rc<RefCell<BotCtx>>,
        outputs: &mut Vec<BotOutput>,
    ) -> GravResult<()> {
        let flow = {
            let st = self.shared.lock().await;
            st.flows.get(name).cloned()
                .ok_or_else(|| runtime_err!("undefined flow '{name}'"))?
        };
        let mut env = Env::new();
        env.define("ctx", Value::Ctx(ctx_rc.clone()));
        match self.exec_block(&flow.body, &mut env, Some(ctx_rc), outputs).await {
            Ok(_) | Err(ExecErr::Return(_)) => Ok(()),
            Err(ExecErr::Err(e)) => Err(e),
            Err(ExecErr::Break | ExecErr::Continue | ExecErr::Stop) => Ok(()),
        }
    }

    /// Returns None if no match, Some(captures) if matched.
    /// captures[0] = full match, captures[1..] = capture groups.
    /// For non-regex patterns, captures is empty on match.
    pub(crate) async fn match_pattern(&self, pattern: &Pattern, val: &Value) -> GravResult<Option<Vec<String>>> {
        match pattern {
            Pattern::Wild => Ok(Some(vec![])),
            Pattern::Lit(expr) => {
                let mut env = Env::new();
                let pv = self.eval_expr(expr, &mut env, None).await?;
                if &pv == val { Ok(Some(vec![])) } else { Ok(None) }
            }
            Pattern::Regex { pattern, flags } => {
                let text = val.to_string();
                let re = {
                    let mut st = self.shared.lock().await;
                    st.get_or_compile_regex(pattern, flags)?
                };
                if let Some(caps) = re.captures(&text) {
                    let mut result = Vec::new();
                    for i in 0..caps.len() {
                        result.push(caps.get(i).map_or("", |m| m.as_str()).to_string());
                    }
                    Ok(Some(result))
                } else {
                    Ok(None)
                }
            }
            Pattern::Bind { name: _, inner } => Box::pin(self.match_pattern(inner, val)).await,
            Pattern::EnumDestruct { enum_name, variant, bindings } => {
                if let Value::Map(m) = val {
                    let m_ref = m.borrow();
                    let val_enum = m_ref.get("__enum__").map(|v| v.to_string()).unwrap_or_default();
                    let val_variant = m_ref.get("__variant__").map(|v| v.to_string()).unwrap_or_default();
                    // Also handle Result type: __result__ field
                    let val_result = m_ref.get("__result__").map(|v| v.to_string());

                    if enum_name == "Result" {
                        // Match Ok(val) / Err(e)
                        let expected_result = match variant.as_str() {
                            "Ok"  => "ok",
                            "Err" => "err",
                            _     => return Ok(None),
                        };
                        if val_result.as_deref() == Some(expected_result) {
                            // Bind captured values
                            // For Ok(val): bind "value" field
                            // For Err(e): bind "error" field
                            // The captures will be bound by the match arm handler
                            // Return empty caps — we'll handle binding via env in the match arm
                            return Ok(Some(vec![]));
                        }
                        return Ok(None);
                    }

                    if val_enum == *enum_name && val_variant == *variant {
                        let _ = bindings; // bindings are bound by match arm handler
                        Ok(Some(vec![]))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
        }
    }

}

#[allow(dead_code)]
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 { return b_len; }
    if b_len == 0 { return a_len; }
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0usize; b_len + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j] + cost)
                .min(curr[j] + 1)
                .min(prev[j + 1] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

impl Interpreter {
    pub(crate) async fn assign_target(
        &self,
        target: &Expr,
        val:    Value,
        env:    &mut Env,
        ctx:    Option<Rc<RefCell<BotCtx>>>,
    ) -> GravResult<()> {
        match target {
            Expr::Var(name) => { env.set(name, val); Ok(()) }
            // Feature 9: metrics.name = / metrics.name +=
            Expr::Field { object, field } if matches!(object.as_ref(), Expr::Var(n) if n == "metrics") => {
                let v = val.as_float().unwrap_or(0.0);
                let mut st = self.shared.lock().await;
                st.bot_metrics.insert(field.clone(), v);
                Ok(())
            }
            Expr::Field { object, field } if matches!(object.as_ref(), Expr::StateRef) => {
                let (user_id, room_id) = ctx.as_ref()
                    .map(|c| { let c = c.borrow(); (c.user_id, c.room_id) })
                    .unwrap_or((0, 0));
                let mut st = self.shared.lock().await;
                st.set_state_field(field, val, user_id, room_id);
                Ok(())
            }
            Expr::Field { object, field } => {
                let obj = Box::pin(self.eval_expr(object, env, ctx)).await?;
                match obj {
                    Value::Map(m) => { m.borrow_mut().insert(field.clone(), val); Ok(()) }
                    _ => Err(crate::runtime_err!("cannot assign to field '{field}'")),
                }
            }
            Expr::Index { object, index } => {
                let obj = Box::pin(self.eval_expr(object, env, ctx.clone())).await?;
                let idx = Box::pin(self.eval_expr(index, env, ctx)).await?;
                match obj {
                    Value::List(l) => {
                        if let Some(i) = idx.as_int() {
                            let mut l = l.borrow_mut();
                            let i = if i < 0 { (l.len() as i64 + i) as usize } else { i as usize };
                            if i < l.len() { l[i] = val; }
                        }
                        Ok(())
                    }
                    Value::Map(m) => { m.borrow_mut().insert(idx.to_string(), val); Ok(()) }
                    _ => Err(crate::runtime_err!("cannot index-assign {}", obj.type_name())),
                }
            }
            _ => Err(crate::runtime_err!("invalid assignment target")),
        }
    }
}
