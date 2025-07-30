use metrics::{counter, describe_counter, describe_histogram, histogram};
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
        describe_counter!(
            "requests_fulfilled_total",
            "Total number of fulfilled randomness requests"
        );
        describe_histogram!(
            "queue_latency_seconds",
            "Time from request creation to fulfillment in seconds"
        );
        describe_counter!(
            "relayer_batch_fulfilled_total",
            "Total number of batch fulfillment transactions"
        );
        describe_histogram!("batch_size", "Size of batches being processed");
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

/// Record a fulfilled request
pub fn record_fulfillment() {
    counter!("requests_fulfilled_total").increment(1);
}

/// Record request latency
pub fn record_latency(latency_seconds: f64) {
    histogram!("queue_latency_seconds").record(latency_seconds);
}

/// Record a batch fulfillment
pub fn record_batch_fulfillment(batch_size: usize) {
    counter!("relayer_batch_fulfilled_total").increment(1);
    histogram!("batch_size").record(batch_size as f64);
}

pub fn record_batch_unfulfilled(batch_size: usize) {
    counter!("relayer_batch_unfulfilled_total").increment(1);
    histogram!("batch_size").record(batch_size as f64);
}
