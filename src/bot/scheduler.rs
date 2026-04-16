use std::sync::Arc;

use crate::ast::TimeUnit;
use crate::interpreter::{Env, Interpreter};
use crate::value::BotOutput;
use super::backend::BotBackend;

pub async fn start_schedulers(
    interp:  &Arc<Interpreter>,
    backend: &Arc<dyn BotBackend>,
) {
    let every_defs = interp.shared.lock().await.every_defs.clone();
    let at_defs    = interp.shared.lock().await.at_defs.clone();

    for ev in every_defs {
        let interp  = Arc::clone(interp);
        let backend = Arc::clone(backend);
        let secs = match ev.unit {
            TimeUnit::Sec  => ev.amount,
            TimeUnit::Min  => ev.amount * 60,
            TimeUnit::Hour => ev.amount * 3600,
            TimeUnit::Day  => ev.amount * 86400,
        };

        tokio::task::spawn_local(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(secs)
            );
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                let mut env = Env::new();
                let mut outputs: Vec<BotOutput> = Vec::new();
                let _ = Box::pin(interp.eval_block_public(&ev.body, &mut env, None, &mut outputs)).await;
                // Broadcast Send outputs to all known rooms; direct outputs go to their room
                let known_rooms = interp.shared.lock().await.known_rooms.clone();
                for out in &outputs {
                    match out {
                        BotOutput::Send { room_id, text } if *room_id == 0 => {
                            // room_id == 0 means broadcast to all known rooms
                            for &rid in &known_rooms {
                                let broadcast = BotOutput::Send { room_id: rid, text: text.clone() };
                                let _ = backend.send_output(&broadcast).await;
                            }
                        }
                        _ => {
                            let _ = backend.send_output(out).await;
                        }
                    }
                }
            }
        });
    }

    for at in at_defs {
        let interp  = Arc::clone(interp);
        let backend = Arc::clone(backend);
        let time_str = at.time.clone();

        tokio::task::spawn_local(async move {
            loop {
                let secs = secs_until(&time_str);
                tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;

                let mut env = Env::new();
                let mut outputs: Vec<BotOutput> = Vec::new();
                let _ = Box::pin(interp.eval_block_public(&at.body, &mut env, None, &mut outputs)).await;
                let known_rooms = interp.shared.lock().await.known_rooms.clone();
                for out in &outputs {
                    match out {
                        BotOutput::Send { room_id, text } if *room_id == 0 => {
                            for &rid in &known_rooms {
                                let broadcast = BotOutput::Send { room_id: rid, text: text.clone() };
                                let _ = backend.send_output(&broadcast).await;
                            }
                        }
                        _ => {
                            let _ = backend.send_output(out).await;
                        }
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(70)).await;
            }
        });
    }
}

pub async fn start_cron_schedulers(
    interp:  &Arc<Interpreter>,
    backend: &Arc<dyn BotBackend>,
) {
    let schedule_defs = interp.shared.lock().await.schedule_defs.clone();

    for sched in schedule_defs {
        let interp  = Arc::clone(interp);
        let backend = Arc::clone(backend);
        let cron_str = sched.cron.clone();
        let body     = sched.body.clone();

        tokio::task::spawn_local(async move {
            use cron::Schedule;
            use std::str::FromStr;

            let schedule = match Schedule::from_str(&cron_str) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[gravitix] invalid cron '{}': {}", cron_str, e);
                    return;
                }
            };

            loop {
                // Find next fire time
                let now = chrono::Utc::now();
                let next = match schedule.after(&now).next() {
                    Some(t) => t,
                    None => {
                        eprintln!("[gravitix] cron '{}' has no future fire times", cron_str);
                        return;
                    }
                };
                let duration = (next - now).to_std().unwrap_or(std::time::Duration::from_secs(60));
                tokio::time::sleep(duration).await;

                let mut env = Env::new();
                let mut outputs: Vec<BotOutput> = Vec::new();
                let _ = Box::pin(interp.eval_block_public(&body, &mut env, None, &mut outputs)).await;

                let known_rooms = interp.shared.lock().await.known_rooms.clone();
                for out in &outputs {
                    match out {
                        BotOutput::Send { room_id, text } if *room_id == 0 => {
                            for &rid in &known_rooms {
                                let broadcast = BotOutput::Send { room_id: rid, text: text.clone() };
                                let _ = backend.send_output(&broadcast).await;
                            }
                        }
                        _ => {
                            let _ = backend.send_output(out).await;
                        }
                    }
                }
            }
        });
    }
}

pub fn secs_until(time_str: &str) -> u64 {
    let parts: Vec<&str> = time_str.split(':').collect();
    let target_h: u64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(9);
    let target_m: u64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    use std::time::{SystemTime, UNIX_EPOCH};
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let day_secs = now_secs % 86400;
    let target_secs = target_h * 3600 + target_m * 60;
    if target_secs > day_secs {
        target_secs - day_secs
    } else {
        86400 - day_secs + target_secs
    }
}
