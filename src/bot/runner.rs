use std::sync::Arc;
use tokio::sync::RwLock;

use crate::ast::Program;
use crate::error::GravResult;
use crate::interpreter::Interpreter;
use crate::value::{BotCtx, UpdateKind};
use super::backend::BotBackend;
use super::telegram::{VortexClient, VortexUpdate};
use super::scheduler::{start_schedulers, start_cron_schedulers};

// ─────────────────────────────────────────────────────────────────────────────
// BotRunner — long-polling loop for Vortex Bot API
// ─────────────────────────────────────────────────────────────────────────────

pub struct BotRunner {
    pub interpreter:   Arc<Interpreter>,
    pub backend:       Arc<dyn BotBackend>,
    /// Shared reference to current program — may be hot-reloaded
    pub program:       Arc<RwLock<Arc<Program>>>,
    vortex_client:     VortexClient,  // for WebSocket access
    /// Path to the script file, used for hot reload
    script_path:       Option<String>,
}

impl BotRunner {
    pub fn new(
        base_url: String,
        token:    String,
        program:  Arc<Program>,
    ) -> Self {
        let vortex_client = VortexClient::new(base_url.clone(), token.clone());
        let backend       = Arc::new(VortexClient::new(base_url.clone(), token.clone()));
        let interpreter   = Arc::new(Interpreter::new(token, base_url));
        Self {
            interpreter,
            backend,
            program: Arc::new(RwLock::new(program)),
            vortex_client,
            script_path: None,
        }
    }

    /// Create a BotRunner from a file path (enables hot-reload via SIGUSR1)
    #[allow(dead_code)]
    pub fn from_file(
        base_url:    String,
        token:       String,
        program:     Arc<Program>,
        script_path: String,
    ) -> Self {
        let vortex_client = VortexClient::new(base_url.clone(), token.clone());
        let backend       = Arc::new(VortexClient::new(base_url.clone(), token.clone()));
        let interpreter   = Arc::new(Interpreter::new(token, base_url));
        Self {
            interpreter,
            backend,
            program: Arc::new(RwLock::new(program)),
            vortex_client,
            script_path: Some(script_path),
        }
    }

    #[allow(dead_code)]
    pub fn with_backend(
        backend:  Arc<dyn BotBackend>,
        token:    String,
        base_url: String,
        program:  Arc<Program>,
    ) -> Self {
        let vortex_client = VortexClient::new(base_url.clone(), token.clone());
        let interpreter = Arc::new(Interpreter::new(token, base_url));
        Self {
            interpreter,
            backend,
            program: Arc::new(RwLock::new(program)),
            vortex_client,
            script_path: None,
        }
    }

    pub async fn run(&self) -> GravResult<()> {
        let current_program = self.program.read().await.clone();
        self.interpreter.load(&current_program).await?;

        let me = self.backend.get_me().await?;
        println!("[gravitix] Bot @{} connected to Vortex.", me.username);

        // Hot reload via SIGUSR1 (Unix only)
        // We use a channel: the signal handler sends on a channel,
        // and the LocalSet task receives and does the actual reload.
        #[cfg(unix)]
        let reload_rx = {
            let (tx, rx) = tokio::sync::mpsc::channel::<()>(1);
            tokio::spawn(async move {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigusr1 = match signal(SignalKind::user_defined1()) {
                    Ok(s) => s,
                    Err(e) => { eprintln!("[gravitix] SIGUSR1 setup failed: {e}"); return; }
                };
                loop {
                    sigusr1.recv().await;
                    let _ = tx.send(()).await;
                }
            });
            rx
        };

        let local = tokio::task::LocalSet::new();
        local.run_until(async {
            start_schedulers(&self.interpreter, &self.backend).await;
            start_cron_schedulers(&self.interpreter, &self.backend).await;

            // Spawn a local task to handle hot-reload signals
            #[cfg(unix)]
            {
                let program_rw   = Arc::clone(&self.program);
                let interp_clone = Arc::clone(&self.interpreter);
                let script_path  = self.script_path.clone();
                let mut reload_rx = reload_rx;
                tokio::task::spawn_local(async move {
                    while reload_rx.recv().await.is_some() {
                        if let Some(ref path) = script_path {
                            match reload_script(path, &program_rw, &interp_clone).await {
                                Ok(_)  => println!("[gravitix] hot-reloaded {path}"),
                                Err(e) => eprintln!("[gravitix] hot-reload failed: {e}"),
                            }
                        }
                    }
                });
            }

            // Try WebSocket first
            match self.vortex_client.connect_ws().await {
                Ok(ws_stream) => {
                    use futures_util::StreamExt;
                    println!("[gravitix] Using WebSocket transport.");
                    let mut stream = Box::pin(ws_stream);
                    loop {
                        match stream.next().await {
                            Some(Ok(upd)) => self.handle_update(upd).await,
                            Some(Err(e)) => {
                                eprintln!("[gravitix] WS error: {e}, falling back to polling");
                                break;
                            }
                            None => break,
                        }
                    }
                }
                Err(_) => {
                    // WebSocket not available, proceed to long-poll
                }
            }

            // Long-poll fallback
            println!("[gravitix] Using long-poll transport.");
            loop {
                match self.backend.get_updates(30).await {
                    Err(e) => {
                        eprintln!("[gravitix] polling error: {e}");
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                    Ok(updates) => {
                        for upd in updates {
                            self.handle_update(upd).await;
                        }
                    }
                }
            }
        }).await
    }

    async fn handle_update(&self, upd: VortexUpdate) {
        let ctx = match build_ctx(&upd) {
            Some(c) => c,
            None => return,
        };
        let update_type = update_type_str(&upd);
        let room_id = ctx.room_id;

        let interp  = Arc::clone(&self.interpreter);
        let program = self.program.read().await.clone();
        let backend = Arc::clone(&self.backend);

        tokio::task::spawn_local(async move {
            let result = tokio::time::timeout(
                tokio::time::Duration::from_secs(30),
                interp.dispatch(&program, ctx, update_type),
            ).await;

            match result {
                Err(_timeout) => eprintln!("[gravitix] handler timed out for update in room {room_id}"),
                Ok(Err(e))    => eprintln!("[gravitix] handler error: {e}"),
                Ok(Ok(outputs)) => {
                    for out in outputs {
                        if let Err(e) = backend.send_output(&out).await {
                            eprintln!("[gravitix] output error: {e}");
                        }
                    }
                }
            }
        });
    }

    /// Public — called from main.rs for scheduler start outside run()
    #[allow(dead_code)]
    pub async fn start_schedulers(&self) {
        start_schedulers(&self.interpreter, &self.backend).await;
        start_cron_schedulers(&self.interpreter, &self.backend).await;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hot reload helper
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(unix)]
async fn reload_script(
    path:       &str,
    program_rw: &Arc<tokio::sync::RwLock<Arc<Program>>>,
    interp:     &Arc<Interpreter>,
) -> crate::error::GravResult<()> {
    let src = std::fs::read_to_string(path)?;
    let tokens = crate::lexer::Lexer::new(&src).tokenize()?;
    let new_prog = crate::parser::Parser::new(tokens).parse()?;
    let new_prog = Arc::new(new_prog);
    interp.load(&new_prog).await?;
    *program_rw.write().await = new_prog;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn build_ctx(upd: &VortexUpdate) -> Option<BotCtx> {
    match upd {
        VortexUpdate::Message { room_id, sender, sender_id, text, message_id, timestamp } => {
            Some(BotCtx {
                room_id:       *room_id,
                user_id:       *sender_id,
                username:      sender.clone(),
                text:          Some(text.clone()),
                message_id:    *message_id,
                command:       None,
                args:          vec![],
                callback_data: None,
                callback_id:   None,
                timestamp:     *timestamp,
                reaction:      None,
                file_url:      None,
                file_size:     None,
                duration:      None,
                is_dm:         false,
                mention_text:  None,
                update_kind:   UpdateKind::Message,
                user_lang:     None,
                webhook_body:    None,
                webhook_headers: None,
                vote_option: None,
                forward_from: None,
                is_thread: false,
                intent: None,
                platform: "vortex".into(),
            })
        }
        VortexUpdate::Command { command, args, room_id, sender, sender_id, message_id, timestamp } => {
            Some(BotCtx {
                room_id:       *room_id,
                user_id:       *sender_id,
                username:      sender.clone(),
                text:          Some(format!("/{} {}", command, args.join(" ")).trim().to_string()),
                message_id:    *message_id,
                command:       Some(command.clone()),
                args:          args.clone(),
                callback_data: None,
                callback_id:   None,
                timestamp:     *timestamp,
                reaction:      None,
                file_url:      None,
                file_size:     None,
                duration:      None,
                is_dm:         false,
                mention_text:  None,
                update_kind:   UpdateKind::Command,
                user_lang:     None,
                webhook_body:    None,
                webhook_headers: None,
                vote_option: None,
                forward_from: None,
                is_thread: false,
                intent: None,
                platform: "vortex".into(),
            })
        }
        VortexUpdate::Callback { callback_id, data, room_id, sender, sender_id, timestamp } => {
            Some(BotCtx {
                room_id:       *room_id,
                user_id:       *sender_id,
                username:      sender.clone(),
                text:          None,
                message_id:    0,
                command:       None,
                args:          vec![],
                callback_data: Some(data.clone()),
                callback_id:   Some(callback_id.clone()),
                timestamp:     *timestamp,
                reaction:      None,
                file_url:      None,
                file_size:     None,
                duration:      None,
                is_dm:         false,
                mention_text:  None,
                update_kind:   UpdateKind::Callback,
                user_lang:     None,
                webhook_body:    None,
                webhook_headers: None,
                vote_option: None,
                forward_from: None,
                is_thread: false,
                intent: None,
                platform: "vortex".into(),
            })
        }
        VortexUpdate::Join { room_id, user_id, username, timestamp } => {
            Some(BotCtx {
                room_id:       *room_id,
                user_id:       *user_id,
                username:      username.clone(),
                text:          None,
                message_id:    0,
                command:       None,
                args:          vec![],
                callback_data: None,
                callback_id:   None,
                timestamp:     *timestamp,
                reaction:      None,
                file_url:      None,
                file_size:     None,
                duration:      None,
                is_dm:         false,
                mention_text:  None,
                update_kind:   UpdateKind::Join,
                user_lang:     None,
                webhook_body:    None,
                webhook_headers: None,
                vote_option: None,
                forward_from: None,
                is_thread: false,
                intent: None,
                platform: "vortex".into(),
            })
        }
        VortexUpdate::Leave { room_id, user_id, username, timestamp } => {
            Some(BotCtx {
                room_id:       *room_id,
                user_id:       *user_id,
                username:      username.clone(),
                text:          None,
                message_id:    0,
                command:       None,
                args:          vec![],
                callback_data: None,
                callback_id:   None,
                timestamp:     *timestamp,
                reaction:      None,
                file_url:      None,
                file_size:     None,
                duration:      None,
                is_dm:         false,
                mention_text:  None,
                update_kind:   UpdateKind::Leave,
                user_lang:     None,
                webhook_body:    None,
                webhook_headers: None,
                vote_option: None,
                forward_from: None,
                is_thread: false,
                intent: None,
                platform: "vortex".into(),
            })
        }
        VortexUpdate::Reaction { room_id, sender, sender_id, emoji, timestamp } => {
            Some(BotCtx {
                room_id:       *room_id,
                user_id:       *sender_id,
                username:      sender.clone(),
                text:          None,
                message_id:    0,
                command:       None,
                args:          vec![],
                callback_data: None,
                callback_id:   None,
                timestamp:     *timestamp,
                reaction:      Some(emoji.clone()),
                file_url:      None,
                file_size:     None,
                duration:      None,
                is_dm:         false,
                mention_text:  None,
                update_kind:   UpdateKind::Reaction,
                user_lang:     None,
                webhook_body:    None,
                webhook_headers: None,
                vote_option: None,
                forward_from: None,
                is_thread: false,
                intent: None,
                platform: "vortex".into(),
            })
        }
    }
}

fn update_type_str(upd: &VortexUpdate) -> &'static str {
    match upd {
        VortexUpdate::Message  { .. } => "message",
        VortexUpdate::Command  { .. } => "command",
        VortexUpdate::Callback { .. } => "callback",
        VortexUpdate::Join     { .. } => "join",
        VortexUpdate::Leave    { .. } => "leave",
        VortexUpdate::Reaction { .. } => "reaction",
    }
}
