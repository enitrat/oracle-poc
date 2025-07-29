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
    
    /// Run both indexer and GraphQL server (default)
    Run {
        /// Custom port for GraphQL server
        #[arg(short, long)]
        port: Option<u16>,
    },
}