use super::zamaoracle::vrf_oracle::vrf_oracle_handlers;
use rindexer::event::callback_registry::EventCallbackRegistry;
use std::path::PathBuf;

pub async fn register_all_handlers(manifest_path: &PathBuf) -> EventCallbackRegistry {
    let mut registry = EventCallbackRegistry::new();
    vrf_oracle_handlers(manifest_path, &mut registry).await;
    registry
}
