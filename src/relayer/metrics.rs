use metrics::{counter, describe_counter};
use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize metrics descriptions
pub fn init_metrics() {
    INIT.call_once(|| {
        describe_counter!(
            "relayer_selected_total",
            "Total number of times a relayer account was selected"
        );
        describe_counter!(
            "relayer_skipped_total",
            "Total number of times a relayer account was skipped"
        );
    });
}

/// Record a successful account selection
pub fn record_selection(address: &str) {
    counter!(
        "relayer_selected_total",
        "address" => address.to_string()
    )
    .increment(1);
}

/// Record a skipped account
pub fn record_skip(address: &str, reason: &str) {
    counter!(
        "relayer_skipped_total",
        "address" => address.to_string(),
        "reason" => reason.to_string()
    )
    .increment(1);
}
