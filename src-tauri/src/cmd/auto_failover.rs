use super::{CmdResult, StringifyErr as _};
use crate::core::auto_failover::{self, FailoverStatus, policy::AutoFailoverConfig};

#[tauri::command]
pub fn get_auto_failover_status() -> FailoverStatus {
    auto_failover::status()
}

#[tauri::command]
pub async fn set_auto_failover_config(config: AutoFailoverConfig) -> CmdResult {
    auto_failover::configure(config).await.stringify_err()
}

#[tauri::command]
pub fn check_auto_failover_now() {
    auto_failover::request_check();
}

#[tauri::command]
pub async fn select_auto_failover_node(group: String, node: String) -> CmdResult<bool> {
    auto_failover::manual_select(&group, &node).await.stringify_err()
}

#[tauri::command]
pub async fn set_entry_pool_config(config: crate::config::entry_pool::EntryPoolConfig) -> CmdResult {
    auto_failover::configure_entry_pool(config).await.stringify_err()
}
