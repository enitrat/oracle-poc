mod account;
mod config;
mod metrics;
mod scheduler;

pub use config::RelayerConfig;
pub use scheduler::Relayer;

#[derive(Debug, Clone)]
pub enum SkipReason {
    InsufficientGas,
    PendingTransaction,
    RecentFailure,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientGas => write!(f, "insufficient_gas"),
            Self::PendingTransaction => write!(f, "pending_transaction"),
            Self::RecentFailure => write!(f, "recent_failure"),
        }
    }
}
