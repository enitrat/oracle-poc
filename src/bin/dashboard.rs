use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dotenvy::dotenv;
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Bar, BarChart, BarGroup, Block, Borders, Chart, Dataset, GraphType, List, ListItem,
        Paragraph, Row, Table,
    },
    Frame, Terminal,
};
use std::env;
use std::sync::Arc;
use std::{
    collections::VecDeque,
    io,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tokio::time::interval;

use zamaoracle::dashboard::data::{DataLayer, Stats, StatsSnapshot};

const HISTORY_SIZE: usize = 120; // 1 minute of history at 500ms intervals
const REFRESH_INTERVAL: Duration = Duration::from_millis(500);
const MAX_ERROR_LOG: usize = 10;

#[derive(Clone)]
struct App {
    stats: Stats,
    history: VecDeque<StatsSnapshot>,
    error_log: VecDeque<(DateTime<Utc>, String)>,
    paused: bool,
    last_update: Instant,
    data_layer: Arc<DataLayer>,
    request_rate: f64,       // requests per minute
    latency_trend: Vec<f64>, // moving average
}

impl App {
    async fn new() -> Result<Self> {
        let data_layer = DataLayer::new().await?;
        let stats = data_layer.get_stats().await.unwrap_or_default();
        let data_layer = Arc::new(data_layer);

        Ok(Self {
            stats,
            history: VecDeque::with_capacity(HISTORY_SIZE),
            error_log: VecDeque::with_capacity(MAX_ERROR_LOG),
            paused: false,
            last_update: Instant::now(),
            data_layer,
            request_rate: 0.0,
            latency_trend: Vec::with_capacity(HISTORY_SIZE),
        })
    }

    async fn update(&mut self) -> Result<()> {
        if self.paused {
            return Ok(());
        }

        match self.data_layer.get_stats().await {
            Ok(stats) => {
                // Calculate request rate
                if let Some(last_snapshot) = self.history.back() {
                    let time_diff = Utc::now()
                        .signed_duration_since(last_snapshot.timestamp)
                        .num_seconds() as f64;

                    if time_diff > 0.0 {
                        let fulfilled_diff = stats
                            .fulfilled_count
                            .saturating_sub(last_snapshot.fulfilled_count);
                        self.request_rate = (fulfilled_diff as f64 / time_diff) * 60.0;
                        // per minute
                    }
                }

                // Update error log if there's a new error
                if let Some(ref error) = stats.last_error {
                    if self.stats.last_error.as_ref() != Some(error) {
                        self.error_log.push_back((Utc::now(), error.clone()));
                        if self.error_log.len() > MAX_ERROR_LOG {
                            self.error_log.pop_front();
                        }
                    }
                }

                self.stats = stats;
                self.last_update = Instant::now();

                // Add to history
                let snapshot = StatsSnapshot {
                    timestamp: Utc::now(),
                    pending_count: self.stats.pending_count,
                    fulfilled_count: self.stats.fulfilled_count,
                    avg_latency: self.stats.avg_latency,
                };

                self.history.push_back(snapshot);
                if self.history.len() > HISTORY_SIZE {
                    self.history.pop_front();
                }

                // Update latency trend (moving average)
                self.latency_trend.push(self.stats.avg_latency);
                if self.latency_trend.len() > HISTORY_SIZE {
                    self.latency_trend.remove(0);
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to fetch stats: {e}");
                self.stats.last_error = Some(error_msg.clone());
                self.error_log.push_back((Utc::now(), error_msg));
                if self.error_log.len() > MAX_ERROR_LOG {
                    self.error_log.pop_front();
                }
            }
        }

        Ok(())
    }

    const fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    // Ensure DATABASE_URL is set
    if env::var("DATABASE_URL").is_err() {
        eprintln!("Error: DATABASE_URL environment variable must be set");
        eprintln!("Example: export DATABASE_URL=postgresql://user:password@localhost/dbname");
        std::process::exit(1);
    }

    // Initialize app
    let app = match App::new().await {
        Ok(app) => Arc::new(Mutex::new(app)),
        Err(e) => {
            eprintln!("\nFailed to initialize dashboard: {e}");
            eprintln!("\nPlease ensure PostgreSQL is running and accessible.");
            std::process::exit(1);
        }
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create a channel for shutdown signal
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Spawn update task
    let app_clone = app.clone();
    let update_handle = tokio::spawn(async move {
        let mut ticker = interval(REFRESH_INTERVAL);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let mut app = app_clone.lock().await;
                    let _ = app.update().await;
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
    });

    // Run the UI
    let res = run_ui(&mut terminal, app).await;

    // Cleanup
    let _ = shutdown_tx.send(()).await;
    update_handle.abort();

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

async fn run_ui<B: Backend>(terminal: &mut Terminal<B>, app: Arc<Mutex<App>>) -> io::Result<()> {
    loop {
        // Draw
        let app_state = app.lock().await.clone();
        terminal.draw(|f| draw_ui(f, &app_state))?;

        // Handle events
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(()),
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            app.lock().await.toggle_pause();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn draw_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Length(7),  // Stats cards
            Constraint::Length(15), // Main charts
            Constraint::Length(8),  // Secondary info
            Constraint::Min(3),     // Status bar
        ])
        .split(f.area());

    // Title with connection status
    draw_title(f, chunks[0], app);

    // Stats cards
    draw_stats_cards(f, chunks[1], app);

    // Main charts area
    let chart_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);

    draw_queue_chart(f, chart_chunks[0], app);
    draw_latency_chart(f, chart_chunks[1], app);

    // Secondary info area
    let info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(chunks[3]);

    draw_relayer_chart(f, info_chunks[0], app);
    draw_relayer_stats_table(f, info_chunks[1], app);
    draw_error_log(f, info_chunks[2], app);

    // Status bar
    draw_status_bar(f, chunks[4], app);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let update_status = if app.paused { "PAUSED" } else { "LIVE" };

    let title_text = vec![Line::from(vec![
        Span::styled(
            "ZamaOracle Dashboard",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            update_status,
            Style::default().fg(if app.paused {
                Color::Yellow
            } else {
                Color::Green
            }),
        ),
        Span::raw(" | "),
        Span::raw(format!(
            "Last Update: {}s ago",
            app.last_update.elapsed().as_secs()
        )),
    ])];

    let title = Paragraph::new(title_text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, area);
}

fn draw_stats_cards(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    // Pending requests
    let pending_val = format!("{}", app.stats.pending_count);
    let pending = create_stat_card("Pending", &pending_val, Color::Yellow);
    f.render_widget(pending, chunks[0]);

    // Fulfilled requests
    let fulfilled_val = format!("{}", app.stats.fulfilled_count);
    let fulfilled = create_stat_card("Fulfilled", &fulfilled_val, Color::Green);
    f.render_widget(fulfilled, chunks[1]);

    // Average latency
    let latency_val = format!("{:.2}s", app.stats.avg_latency);
    let latency = create_stat_card("Avg Latency", &latency_val, Color::Blue);
    f.render_widget(latency, chunks[2]);

    // Failed requests
    let failed_val = format!("{}", app.stats.failed_count);
    let failed = create_stat_card("Failed", &failed_val, Color::Red);
    f.render_widget(failed, chunks[3]);

    // Request rate
    let rate_val = format!("{:.1}", app.request_rate);
    let rate = create_stat_card("Rate/min", &rate_val, Color::Magenta);
    f.render_widget(rate, chunks[4]);
}

fn create_stat_card<'a>(title: &'a str, value: &'a str, color: Color) -> Paragraph<'a> {
    Paragraph::new(vec![
        Line::from(vec![Span::styled(title, Style::default().fg(color))]),
        Line::from(vec![Span::styled(
            value,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
    ])
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center)
}

fn draw_queue_chart(f: &mut Frame, area: Rect, app: &App) {
    if app.history.is_empty() {
        let placeholder = Paragraph::new("No data yet...")
            .block(
                Block::default()
                    .title("Queue Length History")
                    .borders(Borders::ALL),
            )
            .alignment(Alignment::Center);
        f.render_widget(placeholder, area);
        return;
    }

    // Prepare data points
    let data: Vec<(f64, f64)> = app
        .history
        .iter()
        .enumerate()
        .map(|(i, snapshot)| (i as f64, snapshot.pending_count as f64))
        .collect();

    let max_y = data.iter().map(|(_, y)| *y).fold(0.0, f64::max).max(10.0);
    let min_y = 0.0;

    let datasets = vec![Dataset::default()
        .name("Pending")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Yellow))
        .data(&data)];

    let x_labels = vec![Span::raw("60s ago"), Span::raw("30s ago"), Span::raw("now")];

    let y_labels = vec![
        Span::raw(format!("{min_y:.0}")),
        Span::raw(format!("{:.0}", max_y / 2.0)),
        Span::raw(format!("{max_y:.0}")),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title("Queue Length History")
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .title("Time")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, HISTORY_SIZE as f64])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title("Requests")
                .style(Style::default().fg(Color::Gray))
                .bounds([min_y, max_y * 1.1])
                .labels(y_labels),
        );

    f.render_widget(chart, area);
}

fn draw_latency_chart(f: &mut Frame, area: Rect, app: &App) {
    if app.latency_trend.is_empty() {
        let placeholder = Paragraph::new("No latency data yet...")
            .block(
                Block::default()
                    .title("Latency Trend")
                    .borders(Borders::ALL),
            )
            .alignment(Alignment::Center);
        f.render_widget(placeholder, area);
        return;
    }

    // Calculate moving average
    let window_size = 10;
    let mut moving_avg: Vec<(f64, f64)> = Vec::new();

    for i in 0..app.latency_trend.len() {
        let start = i.saturating_sub(window_size / 2);
        let end = (i + window_size / 2 + 1).min(app.latency_trend.len());
        let avg = app.latency_trend[start..end].iter().sum::<f64>() / (end - start) as f64;
        moving_avg.push((i as f64, avg));
    }

    let raw_data: Vec<(f64, f64)> = app
        .latency_trend
        .iter()
        .enumerate()
        .map(|(i, &lat)| (i as f64, lat))
        .collect();

    let max_y = app
        .latency_trend
        .iter()
        .cloned()
        .fold(0.0, f64::max)
        .max(1.0);
    let min_y = 0.0;

    let datasets = vec![
        Dataset::default()
            .name("Raw")
            .marker(symbols::Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Style::default().fg(Color::Blue))
            .data(&raw_data),
        Dataset::default()
            .name("Moving Avg")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&moving_avg),
    ];

    let y_labels = vec![
        Span::raw("0s"),
        Span::raw(format!("{:.1}s", max_y / 2.0)),
        Span::raw(format!("{max_y:.1}s")),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title("Latency Trend")
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .title("Time")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, app.latency_trend.len() as f64]),
        )
        .y_axis(
            Axis::default()
                .title("Seconds")
                .style(Style::default().fg(Color::Gray))
                .bounds([min_y, max_y * 1.1])
                .labels(y_labels),
        );

    f.render_widget(chart, area);
}

fn draw_relayer_stats_table(f: &mut Frame, area: Rect, app: &App) {
    use zamaoracle::dashboard::data::RelayerStats;

    let mut relayer_data: Vec<(String, &RelayerStats)> = app
        .stats
        .relayer_stats
        .iter()
        .map(|(addr, stats)| (addr.clone(), stats))
        .collect();

    // Sort by selected count descending
    relayer_data.sort_by(|a, b| b.1.selected_count.cmp(&a.1.selected_count));

    if relayer_data.is_empty() {
        let placeholder = Paragraph::new("No relayer data")
            .block(
                Block::default()
                    .title("Relayer Stats")
                    .borders(Borders::ALL),
            )
            .alignment(Alignment::Center);
        f.render_widget(placeholder, area);
        return;
    }

    let header =
        Row::new(vec!["Addr", "Txs", "Skip", "Rate"]).style(Style::default().fg(Color::Yellow));

    let rows: Vec<Row> = relayer_data
        .iter()
        .take(4) // Show top 4 relayers
        .map(|(addr, stats)| {
            let short_addr = if addr.len() > 8 {
                format!("{}...", &addr[..8])
            } else {
                addr.clone()
            };

            let total = stats.selected_count + stats.skip_count;
            let success_rate = if total > 0 {
                (stats.selected_count as f64 / total as f64 * 100.0) as u64
            } else {
                0
            };

            Row::new(vec![
                short_addr,
                stats.selected_count.to_string(),
                stats.skip_count.to_string(),
                format!("{}%", success_rate),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        vec![
            Constraint::Percentage(35),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title("Relayer Stats")
            .borders(Borders::ALL),
    )
    .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_widget(table, area);
}

fn draw_relayer_chart(f: &mut Frame, area: Rect, app: &App) {
    let mut data = vec![];

    for (reason, count) in &app.stats.relayer_skips {
        data.push((reason.as_str(), *count));
    }

    if data.is_empty() {
        let placeholder = Paragraph::new("No skip data")
            .block(
                Block::default()
                    .title("Relayer Skips")
                    .borders(Borders::ALL),
            )
            .alignment(Alignment::Center);
        f.render_widget(placeholder, area);
        return;
    }

    // Sort by count descending
    data.sort_by(|a, b| b.1.cmp(&a.1));

    let _max_value = data.iter().map(|(_, v)| *v).max().unwrap_or(1);

    let bars: Vec<Bar> = data
        .iter()
        .take(3) // Show top 3 to fit in smaller space
        .map(|(reason, count)| {
            Bar::default()
                .value(*count)
                .text_value(format!("{count}"))
                .label(Line::from(reason.to_string()))
                .style(Style::default().fg(Color::Red))
        })
        .collect();

    let bar_group = BarGroup::default().bars(&bars);
    let bar_chart = BarChart::default()
        .block(
            Block::default()
                .title("Relayer Skips")
                .borders(Borders::ALL),
        )
        .data(bar_group)
        .bar_width(5)
        .bar_gap(1)
        .value_style(Style::default().fg(Color::White));

    f.render_widget(bar_chart, area);
}

fn draw_error_log(f: &mut Frame, area: Rect, app: &App) {
    let errors: Vec<ListItem> = app
        .error_log
        .iter()
        .rev()
        .take(5)
        .map(|(timestamp, error)| {
            let time_str = timestamp.format("%H:%M:%S").to_string();
            ListItem::new(Line::from(vec![
                Span::styled(time_str, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(error, Style::default().fg(Color::Red)),
            ]))
        })
        .collect();

    let list = List::new(errors).block(
        Block::default()
            .title("Recent Errors")
            .borders(Borders::ALL),
    );

    f.render_widget(list, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Current status
    let status_text = if let Some(error) = &app.stats.last_error {
        vec![Line::from(vec![
            Span::styled("Last Error: ", Style::default().fg(Color::Red)),
            Span::raw(error),
        ])]
    } else {
        vec![Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Green)),
            Span::raw("System running normally | "),
            Span::raw(format!("Selected: {} | ", app.stats.relayer_selected_total)),
            Span::raw(format!(
                "Skip Total: {}",
                app.stats.relayer_skips.values().sum::<u64>()
            )),
        ])]
    };

    let status = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::TOP))
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(status, status_chunks[0]);

    // Controls
    let controls = Paragraph::new(vec![Line::from(vec![
        Span::raw("Press "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" to quit, "),
        Span::styled("p", Style::default().fg(Color::Yellow)),
        Span::raw(" to "),
        Span::raw(if app.paused { "resume" } else { "pause" }),
    ])])
    .alignment(Alignment::Right)
    .block(Block::default().borders(Borders::TOP));
    f.render_widget(controls, status_chunks[1]);
}
