# ZamaOracle Live Dashboard

A real-time terminal dashboard for monitoring ZamaOracle queue and relayer activity.

## Dashboard Versions

### Basic Dashboard (`dashboard`)

The original dashboard with basic metrics display using sparklines and gauges.

### Advanced Dashboard (`dashboard-v2`)

An improved dashboard with:

- Proper charts with axis labels and scaling
- Request rate tracking (requests/minute)
- Latency trend analysis with moving averages
- Network distribution breakdown
- Recent error log viewer
- Better visual layout

## Prerequisites

1. **PostgreSQL Database**: The dashboard requires a running PostgreSQL instance with the ZamaOracle database.
2. **Oracle Running**: For Prometheus metrics, the oracle should be running with metrics enabled.

## Running the Dashboard

```bash
# Set environment variables (optional - these are the defaults)
export DATABASE_URL=postgresql://postgres:postgres@localhost:5432/rindexer
export PROMETHEUS_URL=http://127.0.0.1:9090

# Run the basic dashboard
cargo run --bin dashboard

# Run the advanced dashboard (recommended)
cargo run --bin dashboard-v2

# Or use Make commands
make dashboard      # Basic version
make dashboard-v2   # Advanced version
```

### Starting PostgreSQL with Docker

If you don't have PostgreSQL running:

```bash
docker run -d --name zamaoracle-db \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=rindexer \
  -p 5432:5432 \
  postgres:15
```

## Features

- **Real-time Metrics**: Updates every 500ms
- **Queue Monitoring**: Pending, fulfilled, and failed request counts
- **Latency Tracking**: Average, min, and max latency gauges
- **Relayer Activity**: Skip reasons and selection counts
- **Error Display**: Shows the most recent error message
- **Interactive Controls**: Pause/resume updates, quit gracefully

## UI Layout

### Basic Dashboard

```
+-----------------------------------------------------------+
| ZamaOracle Dashboard                                      |
+-----------------------------------------------------------+
| Pending │ Fulfilled │ Avg Latency │ Failed                |
|  42     │ 1,337     │ 1.37s       │ 3                    |
+-------------------------+-----------------+---------------+
|  Queue Length Sparkline |   Latency Gauge | Relayer Skips |
|  (Last 60s)            |   ████▌ 1.37s   | low_balance:15|
|  ▁▂▃▄▅▆▇█              |   Min: 0.5s     | no_gas: 8     |
+-------------------------+-----------------+---------------+
| Status: System running normally | Press q to quit, p to pause |
+-----------------------------------------------------------+
```

### Dashboard with Relayer Statistics

```
+-------------------------------------------------------------------+
| ZamaOracle Dashboard | LIVE | Last Update: 2s ago                |
+-------------------------------------------------------------------+
| Pending | Fulfilled | Avg Latency | Failed | Rate/min           |
|   42    |   1,337   |    1.37s    |   3    |   12.5             |
+-------------------------------------------------------------------+
|     Queue Length History          |       Latency Trend          |
|  100 ┤                           |   5s ┤     ╱╲                  |
|   50 ┤    ╱╲    ╱╲              |  2.5s┤ ────  ╲──── (avg)       |
|    0 └────────────────           |   0s └─────────────            |
|      60s ago    30s ago    now   |                                |
+---------------------+---------------------+-----------------------+
| Skip Reasons        | Relayer Stats       | Recent Errors         |
| █████ insufficient  | Addr     Txs  Rate  | 14:23:01 Nonce too low|
| ███   pending_tx    | 0x1a2... 142  95%   | 14:22:15 Gas spike    |
| ██    recent_fail   | 0x3b4... 89   87%   | 14:21:33 RPC timeout  |
|                     | 0x5c6... 45   82%   |                       |
+---------------------+---------------------+-----------------------+
| Status: System running normally | Selected: 1337 | Skip Total: 42 |
+-------------------------------------------------------------------+
```

## Keyboard Controls

- `q` or `Q`: Quit the dashboard
- `p` or `P`: Pause/resume updates

## Metrics Displayed

| Metric                    | Description                                      | Source                          |
| ------------------------- | ------------------------------------------------ | ------------------------------- |
| **Pending**               | Number of requests waiting to be processed       | `pending_requests` table        |
| **Fulfilled**             | Total successfully processed requests            | `pending_requests` table        |
| **Avg Latency**           | Average time from request to fulfillment         | Calculated from timestamps      |
| **Failed**                | Number of permanently failed requests            | `pending_requests` table        |
| **Queue Sparkline/Chart** | Visual history of queue length                   | Last 120 samples (60s)          |
| **Latency Gauge/Chart**   | Visual representation of latency trends          | Current avg + moving average    |
| **Relayer Skips**         | Top reasons for skipping relayer accounts        | Prometheus metrics              |
| **Request Rate**          | Fulfilled requests per minute                    | Calculated from fulfilled count |
| **Network Distribution**  | Percentage breakdown by network                  | Database query                  |
| **Error Log**             | Recent error messages with timestamps            | Last N errors from database     |
| **Relayer Stats**         | Per-relayer transaction counts and success rates | Prometheus metrics              |

## Performance

- CPU usage: < 5% while idle
- Memory usage: ~20MB
- Network: Minimal (database queries + Prometheus scraping)
- Terminal: Works on 80×24 and larger terminals

## Troubleshooting

### Connection Refused Error

If you see:

```
Failed to connect to PostgreSQL database!
Connection refused (os error 61)
```

This means PostgreSQL is not running or not accessible. Solutions:

1. Start PostgreSQL (see Docker command above)
2. Check your `DATABASE_URL` environment variable is correct:
   ```bash
   echo $DATABASE_URL
   ```
3. Verify PostgreSQL is running on the expected port:
   ```bash
   # Check if PostgreSQL is listening
   lsof -i :5432  # or your custom port (e.g., 5440)
   ```
4. Test the connection:
   ```bash
   psql "$DATABASE_URL" -c "SELECT 1"
   ```

### Dashboard shows "N/A" for metrics

- Check database connection with `DATABASE_URL`
- Verify Prometheus is running on port 9090 (or the configured `PROMETHEUS_URL`)
- Ensure the oracle is running with metrics enabled (`cargo run -- run` or `cargo run -- --command run`)
- Ensure the `pending_requests` table exists

### High CPU usage

- The refresh interval is set to 500ms by default
- Database queries may need optimization for large datasets

### Missing Prometheus metrics

- Ensure the oracle is running with metrics enabled (`cargo run -- run` includes metrics)
- The oracle exposes metrics on `http://127.0.0.1:9090/metrics`
- Check that the `PROMETHEUS_URL` environment variable is correct if not using defaults
