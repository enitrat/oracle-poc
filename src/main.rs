use std::env;
use clap::Parser;

use self::rindexer_lib::indexers::all_handlers::register_all_handlers;
use rindexer::{
    event::callback_registry::TraceCallbackRegistry, start_rindexer, GraphqlOverrideSettings,
    IndexingDetails, StartDetails,
};
use tracing::info;

mod cli;
mod oracle;
mod rindexer_lib;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let (enable_graphql, enable_indexer, port) = match &cli.command {
        Some(Commands::Indexer { graphql }) => (*graphql, true, cli.port),
        Some(Commands::Graphql { port }) => (true, false, port.or(cli.port)),
        Some(Commands::Run { port }) => (true, true, port.or(cli.port)),
        None => (true, true, cli.port), // Default to running both
    };

    info!(
        "Starting ZamaOracle - Indexer: {}, GraphQL: {}, Port: {:?}",
        enable_indexer, enable_graphql, port
    );

    let path = env::current_dir();
    match path {
        Ok(path) => {
            let manifest_path = path.join("rindexer.yaml");
            let result = start_rindexer(StartDetails {
                manifest_path: &manifest_path,
                indexing_details: if enable_indexer {
                    Some(IndexingDetails {
                        registry: register_all_handlers(&manifest_path).await,
                        trace_registry: TraceCallbackRegistry { events: vec![] },
                    })
                } else {
                    None
                },
                graphql_details: GraphqlOverrideSettings {
                    enabled: enable_graphql,
                    override_port: port,
                },
            })
            .await;

            match result {
                Ok(_) => {}
                Err(e) => {
                    println!("Error starting rindexer: {e:?}");
                }
            }
        }
        Err(e) => {
            println!("Error getting current directory: {e:?}");
        }
    }
}
