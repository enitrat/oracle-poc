use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "zamaoracle")]
#[command(author, version, about = "VRF Oracle for Ethereum", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Override the default port for GraphQL
    #[arg(short, long, global = true)]
    pub port: Option<u16>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the indexer to listen for randomness requests
    Indexer {
        /// Enable GraphQL alongside indexer
        #[arg(short, long)]
        graphql: bool,
    },

    /// Run the GraphQL server only
    Graphql {
        /// Custom port for GraphQL server
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Run indexer, GraphQL server, and queue processor
    Run {
        /// Custom port for GraphQL server
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Run the queue processor to fulfill pending randomness requests
    QueueProcessor {
        /// Poll interval in seconds (default: 5)
        #[arg(long, default_value = "5")]
        poll_interval: u64,

        /// Run migrations before starting
        #[arg(short, long)]
        migrate: bool,
    },
}
