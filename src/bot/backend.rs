use async_trait::async_trait;
use serde::Deserialize;

use crate::error::GravResult;
use crate::value::BotOutput;
use super::telegram::VortexUpdate;

#[derive(Debug, Clone, Deserialize)]
pub struct BotInfo {
    #[allow(dead_code)]
    pub id:       i64,
    pub username: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub name:     String,
}

// ─────────────────────────────────────────────────────────────────────────────
// BotBackend trait — Open/Closed Principle
// New backends implement this without modifying BotRunner.
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait(?Send)]
pub trait BotBackend: Send + Sync {
    async fn get_updates(&self, timeout: u64) -> GravResult<Vec<VortexUpdate>>;
    async fn send_output(&self, output: &BotOutput) -> GravResult<()>;
    async fn get_me(&self) -> GravResult<BotInfo>;
}
