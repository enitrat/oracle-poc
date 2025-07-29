use clap::Parser;
use std::env;

use self::rindexer_lib::indexers::all_handlers::register_all_handlers;
use dotenvy::dotenv;
use rindexer::{
    event::callback_registry::TraceCallbackRegistry, start_rindexer, GraphqlOverrideSettings,
    IndexingDetails, StartDetails,
};
use tracing::{error, info, warn};

mod cli;
mod database;
mod oracle;
mod provider;
mod queue_processor;
mod relayer;
mod rindexer_lib;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    dotenv().ok();
    // Initialize tracing
    tracing_subscriber::fmt::init();

    match &cli.command {
        Some(Commands::QueueProcessor {
            poll_interval,
            migrate,
        }) => {
            info!("Starting ZamaOracle Queue Processor");

            // Ensure DATABASE_URL is set (rindexer will use it internally)
            if env::var("DATABASE_URL").is_err() {
                eprintln!("Error: DATABASE_URL environment variable must be set");
                eprintln!(
                    "Example: export DATABASE_URL=postgresql://user:password@localhost/dbname"
                );
                std::process::exit(1);
            }

            // Create PostgreSQL client
            let postgres_client = match queue_processor::create_postgres_client().await {
                Ok(client) => client,
                Err(e) => {
                    eprintln!("Failed to connect to database: {e:?}");
                    std::process::exit(1);
                }
            };

            let mut processor =
                queue_processor::QueueProcessor::new(postgres_client, *poll_interval * 1000); // Convert seconds to milliseconds

            // Run migrations if requested
            if *migrate {
                info!("Running database migrations...");
                if let Err(e) = processor.run_migrations().await {
                    eprintln!("Failed to run migrations: {e:?}");
                    std::process::exit(1);
                }
                info!("Migrations completed successfully");
            }

            // Start processing queue
            if let Err(e) = processor.start().await {
                eprintln!("Queue processor error: {e:?}");
                std::process::exit(1);
            }
        }
        _ => {
            // Handle other commands (indexer, graphql, run)
            let (enable_graphql, enable_indexer, port, enable_queue_processor, enable_metrics) =
                match &cli.command {
                    Some(Commands::Indexer { graphql }) => (*graphql, true, cli.port, false, false),
                    Some(Commands::Graphql { port }) => {
                        (true, false, port.or(cli.port), false, false)
                    }
                    Some(Commands::Run { port }) => (true, true, port.or(cli.port), true, true),
                    None => (true, true, cli.port, true, true), // Default to running all services
                    _ => unreachable!(),
                };

            info!(
                "Starting ZamaOracle - Indexer: {}, GraphQL: {}, Queue: {}, Metrics: {}, Port: {:?}",
                enable_indexer, enable_graphql, enable_queue_processor, enable_metrics, port
            );

            // Spawn metrics server if enabled
            if enable_metrics {
                tokio::spawn(async {
                    info!("Starting Prometheus metrics server on :9090/metrics");

                    // Initialize metrics exporter
                    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
                    builder
                        .with_http_listener(([0, 0, 0, 0], 9090))
                        .install()
                        .expect("Failed to install Prometheus metrics exporter");

                    // Keep the server running
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                    }
                });
            }

            // Spawn queue processor if enabled
            if enable_queue_processor {
                // Check if DATABASE_URL is set
                if env::var("DATABASE_URL").is_ok() {
                    tokio::spawn(async {
                        info!("Starting Queue Processor in background");

                        // Create PostgreSQL client
                        match queue_processor::create_postgres_client().await {
                            Ok(postgres_client) => {
                                let mut processor =
                                    queue_processor::QueueProcessor::new(postgres_client, 100); // Default 100ms poll interval

                                // Run migrations
                                if let Err(e) = processor.run_migrations().await {
                                    error!("Failed to run queue processor migrations: {:?}", e);
                                    return;
                                }

                                // Start processing
                                if let Err(e) = processor.start().await {
                                    error!("Queue processor error: {:?}", e);
                                }
                            }
                            Err(e) => {
                                error!(
                                    "Failed to create queue processor database connection: {:?}",
                                    e
                                );
                            }
                        }
                    });
                } else {
                    warn!("DATABASE_URL not set, queue processor will not start. Set DATABASE_URL to enable queue processing.");
                }
            }

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
    }
}
