use anyhow::Result;
use chrono::Utc;
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
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Gauge, Paragraph, Sparkline},
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

#[derive(Clone)]
struct App {
    stats: Stats,
    history: VecDeque<StatsSnapshot>,
    paused: bool,
    last_update: Instant,
    data_layer: Arc<DataLayer>,
}

impl App {
    async fn new() -> Result<Self> {
        let data_layer = DataLayer::new().await?;
        let stats = data_layer.get_stats().await.unwrap_or_default();
        let data_layer = Arc::new(data_layer);

        Ok(Self {
            stats,
            history: VecDeque::with_capacity(HISTORY_SIZE),
            paused: false,
            last_update: Instant::now(),
            data_layer,
        })
    }

    async fn update(&mut self) -> Result<()> {
        if self.paused {
            return Ok(());
        }

        match self.data_layer.get_stats().await {
            Ok(stats) => {
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
            }
            Err(e) => {
                self.stats.last_error = Some(format!("Failed to fetch stats: {}", e));
            }
        }

        Ok(())
    }

    fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    // Ensure DATABASE_URL is set (rindexer will use it internally)
    if env::var("DATABASE_URL").is_err() {
        eprintln!("Error: DATABASE_URL environment variable must be set");
        eprintln!("Example: export DATABASE_URL=postgresql://user:password@localhost/dbname");
        std::process::exit(1);
    }

    // Initialize app with better error handling
    let app = match App::new().await {
        Ok(app) => Arc::new(Mutex::new(app)),
        Err(e) => {
            eprintln!("\nFailed to initialize dashboard: {}", e);
            eprintln!("\nPlease ensure PostgreSQL is running and accessible.");
            eprintln!(
                "You can also set DATABASE_URL environment variable if using non-default settings."
            );
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
            Constraint::Length(3), // Title
            Constraint::Length(7), // Stats cards
            Constraint::Min(10),   // Charts
            Constraint::Length(3), // Status bar
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("ZamaOracle Dashboard")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Stats cards
    draw_stats_cards(f, chunks[1], app);

    // Charts area
    let chart_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(34),
        ])
        .split(chunks[2]);

    draw_queue_sparkline(f, chart_chunks[0], app);
    draw_latency_gauge(f, chart_chunks[1], app);
    draw_relayer_chart(f, chart_chunks[2], app);

    // Status bar
    draw_status_bar(f, chunks[3], app);
}

fn draw_stats_cards(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    // Pending requests
    let pending = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            "Pending",
            Style::default().fg(Color::Yellow),
        )]),
        Line::from(vec![Span::styled(
            format!("{}", app.stats.pending_count),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
    ])
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);
    f.render_widget(pending, chunks[0]);

    // Fulfilled requests
    let fulfilled = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            "Fulfilled",
            Style::default().fg(Color::Green),
        )]),
        Line::from(vec![Span::styled(
            format!("{}", app.stats.fulfilled_count),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
    ])
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);
    f.render_widget(fulfilled, chunks[1]);

    // Average latency
    let latency = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            "Avg Latency",
            Style::default().fg(Color::Blue),
        )]),
        Line::from(vec![Span::styled(
            format!("{:.2}s", app.stats.avg_latency),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
    ])
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);
    f.render_widget(latency, chunks[2]);

    // Failed requests
    let failed = Paragraph::new(vec![
        Line::from(vec![Span::styled(
            "Failed",
            Style::default().fg(Color::Red),
        )]),
        Line::from(vec![Span::styled(
            format!("{}", app.stats.failed_count),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
    ])
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);
    f.render_widget(failed, chunks[3]);
}

fn draw_queue_sparkline(f: &mut Frame, area: Rect, app: &App) {
    let data: Vec<u64> = app.history.iter().map(|s| s.pending_count).collect();

    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .title("Queue Length (Last 60s)")
                .borders(Borders::ALL),
        )
        .data(&data)
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(sparkline, area);
}

fn draw_latency_gauge(f: &mut Frame, area: Rect, app: &App) {
    let gauge_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Calculate gauge percentage (assuming max latency of 10s for visualization)
    let percentage = ((app.stats.avg_latency / 10.0) * 100.0).min(100.0) as u16;

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title("Average Latency")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Blue).bg(Color::Black))
        .percent(percentage)
        .label(format!("{:.2}s", app.stats.avg_latency));
    f.render_widget(gauge, gauge_area[0]);

    // Min/Max latency info
    let latency_info = Paragraph::new(vec![Line::from(vec![
        Span::raw("Min: "),
        Span::styled(
            format!("{:.2}s", app.stats.min_latency),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" | Max: "),
        Span::styled(
            format!("{:.2}s", app.stats.max_latency),
            Style::default().fg(Color::Red),
        ),
    ])])
    .alignment(Alignment::Center);
    f.render_widget(latency_info, gauge_area[1]);
}

fn draw_relayer_chart(f: &mut Frame, area: Rect, app: &App) {
    let mut data = vec![];

    for (reason, count) in &app.stats.relayer_skips {
        data.push((reason.as_str(), *count));
    }

    // Sort by count descending
    data.sort_by(|a, b| b.1.cmp(&a.1));

    let bars: Vec<Bar> = data
        .iter()
        .take(5) // Show top 5 skip reasons
        .map(|(reason, count)| {
            Bar::default()
                .value(*count)
                .label(Line::from(reason.to_string()))
                .style(Style::default().fg(Color::Red))
        })
        .collect();

    let bar_group = BarGroup::default().bars(&bars);
    let bar_chart = BarChart::default()
        .block(
            Block::default()
                .title("Relayer Skip Reasons")
                .borders(Borders::ALL),
        )
        .data(bar_group)
        .bar_width(3)
        .bar_gap(1)
        .value_style(Style::default().fg(Color::White));

    f.render_widget(bar_chart, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Error message or status
    let status_text = if let Some(error) = &app.stats.last_error {
        vec![Line::from(vec![
            Span::styled("Last Error: ", Style::default().fg(Color::Red)),
            Span::raw(error),
        ])]
    } else {
        vec![Line::from(vec![Span::styled(
            "System running normally",
            Style::default().fg(Color::Green),
        )])]
    };

    let status = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::TOP))
        .wrap(ratatui::widgets::Wrap { trim: true });
    f.render_widget(status, status_chunks[0]);

    // Controls info
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
