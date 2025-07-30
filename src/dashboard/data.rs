use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest;
use std::collections::HashMap;
use tokio_postgres::{Client, NoTls};

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub pending_count: u64,
    pub fulfilled_count: u64,
    pub failed_count: u64,
    pub avg_latency: f64,
    pub min_latency: f64,
    pub max_latency: f64,
    pub relayer_selected_total: u64,
    pub relayer_skips: HashMap<String, u64>,
    pub relayer_stats: HashMap<String, RelayerStats>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RelayerStats {
    pub selected_count: u64,
    pub skip_count: u64,
    pub skip_reasons: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct StatsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub pending_count: u64,
    pub fulfilled_count: u64,
    pub avg_latency: f64,
}

pub struct DataLayer {
    pub pg_client: Client,
    pub prometheus_url: String,
}

impl DataLayer {
    pub async fn new() -> Result<Self> {
        // Get database URL from environment
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| {
                eprintln!("DATABASE_URL not set, using default: postgresql://postgres:postgres@localhost:5432/rindexer");
                "postgresql://postgres:postgres@localhost:5432/rindexer".to_string()
            });

        eprintln!("Attempting to connect to PostgreSQL at: {database_url}");

        // Connect to PostgreSQL with better error handling
        let (client, connection) = match tokio_postgres::connect(&database_url, NoTls).await {
            Ok(result) => result,
            Err(e) => {
                eprintln!("\nFailed to connect to PostgreSQL database!");
                eprintln!("Connection string: {database_url}");
                eprintln!("Error: {e}");
                eprintln!("\nPlease ensure:");
                eprintln!("1. PostgreSQL is running");
                eprintln!("2. The database exists");
                eprintln!("3. The DATABASE_URL environment variable is correct");
                eprintln!("\nExample Docker command to start PostgreSQL:");
                eprintln!("docker run -d --name zamaoracle-db -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=rindexer -p 5432:5432 postgres:15");
                return Err(e.into());
            }
        };

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("PostgreSQL connection error: {e}");
            }
        });

        // Get Prometheus URL from environment, defaulting to the same port as main app
        let prometheus_url =
            std::env::var("PROMETHEUS_URL").unwrap_or_else(|_| "http://127.0.0.1:9090".to_string());

        Ok(Self {
            pg_client: client,
            prometheus_url,
        })
    }

    pub async fn get_stats(&self) -> Result<Stats> {
        let mut stats = Stats::default();

        // Get PostgreSQL stats
        if let Ok(pg_stats) = self.get_postgres_stats().await {
            stats.pending_count = pg_stats.0;
            stats.fulfilled_count = pg_stats.1;
            stats.failed_count = pg_stats.2;
            stats.avg_latency = pg_stats.3;
            stats.min_latency = pg_stats.4;
            stats.max_latency = pg_stats.5;
            stats.last_error = pg_stats.6;
        }

        // Get Prometheus metrics
        if let Ok(prom_stats) = self.get_prometheus_stats().await {
            stats.relayer_selected_total = prom_stats.0;
            stats.relayer_skips = prom_stats.1;
        }

        // Get per-relayer statistics
        if let Ok(relayer_stats) = self.get_relayer_stats().await {
            stats.relayer_stats = relayer_stats;
        }

        Ok(stats)
    }

    pub async fn get_network_breakdown(&self) -> Result<Vec<(String, u64)>> {
        let query = r#"
            SELECT network, COUNT(*) as count
            FROM zamaoracle_vrf_oracle.pending_requests
            WHERE status IN ('pending', 'processing', 'fulfilled')
            GROUP BY network
            ORDER BY count DESC
        "#;

        let rows = self.pg_client.query(query, &[]).await?;
        let mut results = Vec::new();

        for row in rows {
            let network: String = row.get(0);
            let count: i64 = row.get(1);
            results.push((network, count as u64));
        }

        Ok(results)
    }

    pub async fn get_recent_errors(&self, limit: i64) -> Result<Vec<(DateTime<Utc>, String)>> {
        let query = r#"
            SELECT updated_at, last_error
            FROM zamaoracle_vrf_oracle.pending_requests
            WHERE last_error IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT $1
        "#;

        let rows = self.pg_client.query(query, &[&limit]).await?;
        let mut results = Vec::new();

        for row in rows {
            let timestamp: chrono::DateTime<Utc> = row.get(0);
            let error: String = row.get(1);
            results.push((timestamp, error));
        }

        Ok(results)
    }

    async fn get_postgres_stats(&self) -> Result<(u64, u64, u64, f64, f64, f64, Option<String>)> {
        // Query for counts
        let count_query = r#"
            SELECT
                COUNT(*) FILTER (WHERE status = 'pending') as pending,
                COUNT(*) FILTER (WHERE status = 'fulfilled') as fulfilled,
                COUNT(*) FILTER (WHERE status = 'failed') as failed
            FROM zamaoracle_vrf_oracle.pending_requests
        "#;

        let count_row = self.pg_client.query_one(count_query, &[]).await?;
        let pending_count: i64 = count_row.get(0);
        let fulfilled_count: i64 = count_row.get(1);
        let failed_count: i64 = count_row.get(2);

        // Query for latency stats (only for fulfilled requests)
        let latency_query = r#"
            SELECT
                COALESCE(AVG(EXTRACT(EPOCH FROM (COALESCE(fulfilled_at, updated_at) - created_at))), 0) as avg_latency,
                COALESCE(MIN(EXTRACT(EPOCH FROM (COALESCE(fulfilled_at, updated_at) - created_at))), 0) as min_latency,
                COALESCE(MAX(EXTRACT(EPOCH FROM (COALESCE(fulfilled_at, updated_at) - created_at))), 0) as max_latency
            FROM zamaoracle_vrf_oracle.pending_requests
            WHERE status = 'fulfilled' AND COALESCE(fulfilled_at, updated_at) > created_at
        "#;

        let latency_row = self.pg_client.query_one(latency_query, &[]).await?;
        let avg_latency_ms: rust_decimal::Decimal = latency_row.get(0);
        let min_latency_ms: rust_decimal::Decimal = latency_row.get(1);
        let max_latency_ms: rust_decimal::Decimal = latency_row.get(2);

        // Query for last error
        let error_query = r#"
            SELECT last_error
            FROM zamaoracle_vrf_oracle.pending_requests
            WHERE last_error IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT 1
        "#;

        let last_error = match self.pg_client.query_opt(error_query, &[]).await? {
            Some(row) => row.get(0),
            None => None,
        };

        let avg_latency: f64 = avg_latency_ms.try_into().unwrap();
        let min_latency: f64 = min_latency_ms.try_into().unwrap();
        let max_latency: f64 = max_latency_ms.try_into().unwrap();

        Ok((
            pending_count as u64,
            fulfilled_count as u64,
            failed_count as u64,
            avg_latency,
            min_latency,
            max_latency,
            last_error,
        ))
    }

    async fn get_prometheus_stats(&self) -> Result<(u64, HashMap<String, u64>)> {
        // Directly scrape the metrics endpoint exposed by the oracle
        // The oracle runs its own metrics server on port 9090 by default
        let metrics_url = format!("{}/metrics", self.prometheus_url);

        let response = reqwest::get(&metrics_url).await?;
        let body = response.text().await?;

        let mut relayer_selected_total = 0u64;
        let mut relayer_skips = HashMap::new();

        for line in body.lines() {
            if line.starts_with("relayer_selected_total") {
                if let Some(value) = parse_metric_value(line) {
                    relayer_selected_total += value;
                }
            } else if line.starts_with("relayer_skipped_total") {
                if let Some((labels, value)) = parse_metric_with_labels(line) {
                    if let Some(reason) = extract_label_value(&labels, "reason") {
                        *relayer_skips.entry(reason).or_insert(0) += value;
                    }
                }
            }
        }

        Ok((relayer_selected_total, relayer_skips))
    }

    pub async fn get_relayer_stats(&self) -> Result<HashMap<String, RelayerStats>> {
        let metrics_url = format!("{}/metrics", self.prometheus_url);

        let response = reqwest::get(&metrics_url).await?;
        let body = response.text().await?;

        let mut relayer_stats: HashMap<String, RelayerStats> = HashMap::new();

        for line in body.lines() {
            if line.starts_with("relayer_selected_total") {
                if let Some((labels, value)) = parse_metric_with_labels(line) {
                    if let Some(address) = extract_label_value(&labels, "address") {
                        relayer_stats.entry(address).or_default().selected_count = value;
                    }
                }
            } else if line.starts_with("relayer_skipped_total") {
                if let Some((labels, value)) = parse_metric_with_labels(line) {
                    if let (Some(address), Some(reason)) = (
                        extract_label_value(&labels, "address"),
                        extract_label_value(&labels, "reason"),
                    ) {
                        let stats = relayer_stats.entry(address).or_default();
                        stats.skip_count += value;
                        *stats.skip_reasons.entry(reason).or_insert(0) = value;
                    }
                }
            }
        }

        Ok(relayer_stats)
    }
}

fn parse_metric_value(line: &str) -> Option<u64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].parse().ok()
    } else {
        None
    }
}

fn parse_metric_with_labels(line: &str) -> Option<(String, u64)> {
    if let Some(label_start) = line.find('{') {
        if let Some(label_end) = line.find('}') {
            let labels = line[label_start + 1..label_end].to_string();
            let rest = &line[label_end + 1..];
            if let Some(value) = rest.split_whitespace().next() {
                if let Ok(val) = value.parse() {
                    return Some((labels, val));
                }
            }
        }
    }
    None
}

fn extract_label_value(labels: &str, label_name: &str) -> Option<String> {
    for part in labels.split(',') {
        let kv: Vec<&str> = part.split('=').collect();
        if kv.len() == 2 && kv[0].trim() == label_name {
            return Some(kv[1].trim().trim_matches('"').to_string());
        }
    }
    None
}
