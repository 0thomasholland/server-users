use anyhow::Result;
use chrono::{DateTime, Local};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph, Row, Table,
    },
    Frame, Terminal,
};
use ssh2::Session;
use std::{
    collections::HashMap,
    io::{self, Read},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_HISTORY: usize = 100;

#[derive(Clone, Debug)]
struct UserStats {
    username: String,
    cpu_percent: f64,
    ram_mb: f64,
    last_updated: DateTime<Local>,
}

#[derive(Clone, Debug)]
struct HistoricalData {
    timestamp: DateTime<Local>,
    cpu_total: f64,
    ram_total: f64,
}

struct App {
    users: Vec<UserStats>,
    history: Vec<HistoricalData>,
    selected_user: usize,
    should_quit: bool,
}

impl App {
    fn new() -> App {
        App {
            users: Vec::new(),
            history: Vec::new(),
            selected_user: 0,
            should_quit: false,
        }
    }

    fn update_data(&mut self, users: Vec<UserStats>) {
        self.users = users;
        
        // Calculate totals for history
        let cpu_total: f64 = self.users.iter().map(|u| u.cpu_percent).sum();
        let ram_total: f64 = self.users.iter().map(|u| u.ram_mb).sum();
        
        self.history.push(HistoricalData {
            timestamp: Local::now(),
            cpu_total,
            ram_total,
        });
        
        // Keep only last MAX_HISTORY entries
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }

    fn next_user(&mut self) {
        if !self.users.is_empty() {
            self.selected_user = (self.selected_user + 1) % self.users.len();
        }
    }

    fn previous_user(&mut self) {
        if !self.users.is_empty() {
            if self.selected_user > 0 {
                self.selected_user -= 1;
            } else {
                self.selected_user = self.users.len() - 1;
            }
        }
    }
}

fn ssh_get_user_stats(host: &str, user: &str, password: &str) -> Result<Vec<UserStats>> {
    let tcp = TcpStream::connect(format!("{}:22", host))?;
    let mut sess = Session::new()?;
    sess.set_tcp_stream(tcp);
    sess.handshake()?;
    sess.userauth_password(user, password)?;

    // Get per-user CPU and memory usage
    let mut channel = sess.channel_session()?;
    
    // This command gets CPU and memory usage per user
    // Uses ps to get processes with user, CPU%, and memory
    let cmd = r#"ps aux | awk 'NR>1 {cpu[$1]+=$3; mem[$1]+=$4; rss[$1]+=$6} END {for(user in cpu) printf "%s %.2f %.2f\n", user, cpu[user], rss[user]/1024}'"#;
    
    channel.exec(cmd)?;
    let mut output = String::new();
    channel.read_to_string(&mut output)?;
    channel.wait_close()?;

    let now = Local::now();
    let mut users = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            users.push(UserStats {
                username: parts[0].to_string(),
                cpu_percent: parts[1].parse().unwrap_or(0.0),
                ram_mb: parts[2].parse().unwrap_or(0.0),
                last_updated: now,
            });
        }
    }

    // Sort by CPU usage (descending)
    users.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap());

    Ok(users)
}

fn ui<B: Backend>(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(12),
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("SSH Server Monitor - User CPU & RAM Usage")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Middle section: split into table and current stats
    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[1]);

    // User table
    let header = Row::new(vec!["User", "CPU %", "RAM (MB)", "Last Updated"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = app.users.iter().enumerate().map(|(i, user)| {
        let style = if i == app.selected_user {
            Style::default().fg(Color::Black).bg(Color::LightCyan)
        } else {
            Style::default()
        };
        
        Row::new(vec![
            user.username.clone(),
            format!("{:.2}", user.cpu_percent),
            format!("{:.2}", user.ram_mb),
            user.last_updated.format("%H:%M:%S").to_string(),
        ])
        .style(style)
    }).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Users"));

    f.render_widget(table, middle_chunks[0]);

    // Current stats summary
    let cpu_total: f64 = app.users.iter().map(|u| u.cpu_percent).sum();
    let ram_total: f64 = app.users.iter().map(|u| u.ram_mb).sum();
    
    let stats_text = vec![
        Line::from(vec![
            Span::styled("Total Users: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", app.users.len())),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Total CPU: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{:.2}%", cpu_total)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Total RAM: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{:.2} MB", ram_total)),
        ]),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::styled("Controls:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("↑/↓: Select user"),
        Line::from("q: Quit"),
    ];

    let stats = Paragraph::new(stats_text)
        .block(Block::default().borders(Borders::ALL).title("Summary"));
    f.render_widget(stats, middle_chunks[1]);

    // Historical graphs
    let graph_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);

    // CPU graph
    if !app.history.is_empty() {
        let cpu_data: Vec<(f64, f64)> = app
            .history
            .iter()
            .enumerate()
            .map(|(i, h)| (i as f64, h.cpu_total))
            .collect();

        let max_cpu = app
            .history
            .iter()
            .map(|h| h.cpu_total)
            .fold(0.0, f64::max)
            .max(10.0);

        let cpu_dataset = Dataset::default()
            .name("CPU %")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Green))
            .data(&cpu_data);

        let cpu_chart = Chart::new(vec![cpu_dataset])
            .block(Block::default().title("CPU Usage Over Time").borders(Borders::ALL))
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, MAX_HISTORY as f64]),
            )
            .y_axis(
                Axis::default()
                    .title("CPU %")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, max_cpu * 1.1])
                    .labels(vec![
                        "0".into(),
                        format!("{:.0}", max_cpu * 0.5).into(),
                        format!("{:.0}", max_cpu).into(),
                    ]),
            );

        f.render_widget(cpu_chart, graph_chunks[0]);
    }

    // RAM graph
    if !app.history.is_empty() {
        let ram_data: Vec<(f64, f64)> = app
            .history
            .iter()
            .enumerate()
            .map(|(i, h)| (i as f64, h.ram_total))
            .collect();

        let max_ram = app
            .history
            .iter()
            .map(|h| h.ram_total)
            .fold(0.0, f64::max)
            .max(100.0);

        let ram_dataset = Dataset::default()
            .name("RAM MB")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Magenta))
            .data(&ram_data);

        let ram_chart = Chart::new(vec![ram_dataset])
            .block(Block::default().title("RAM Usage Over Time").borders(Borders::ALL))
            .x_axis(
                Axis::default()
                    .title("Time")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, MAX_HISTORY as f64]),
            )
            .y_axis(
                Axis::default()
                    .title("RAM (MB)")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, max_ram * 1.1])
                    .labels(vec![
                        "0".into(),
                        format!("{:.0}", max_ram * 0.5).into(),
                        format!("{:.0}", max_ram).into(),
                    ]),
            );

        f.render_widget(ram_chart, graph_chunks[1]);
    }
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: Arc<Mutex<App>>,
    host: String,
    user: String,
    password: String,
) -> Result<()> {
    // Spawn data collection thread
    let app_clone = app.clone();
    std::thread::spawn(move || loop {
        match ssh_get_user_stats(&host, &user, &password) {
            Ok(users) => {
                let mut app = app_clone.lock().unwrap();
                app.update_data(users);
            }
            Err(e) => {
                eprintln!("Error fetching stats: {}", e);
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    });

    loop {
        {
            let app_guard = app.lock().unwrap();
            terminal.draw(|f| ui(f, &app_guard))?;
            
            if app_guard.should_quit {
                break;
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                let mut app_guard = app.lock().unwrap();
                match key.code {
                    KeyCode::Char('q') => app_guard.should_quit = true,
                    KeyCode::Down => app_guard.next_user(),
                    KeyCode::Up => app_guard.previous_user(),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    // TODO: Replace with your SSH server details
    // You can also read these from command line args or a config file
    let host = "your.server.com".to_string();
    let user = "your_username".to_string();
    let password = "your_password".to_string();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Arc::new(Mutex::new(App::new()));
    let res = run_app(&mut terminal, app, host, user, password);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

