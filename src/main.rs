use anyhow::Result;
use chrono::{DateTime, Local};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph, Row, Table,
    },
    Frame, Terminal,
};
use ssh2::Session;
use std::{
    io::{self, Read},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_HISTORY: usize = 100;

#[derive(Clone, Debug, PartialEq)]
enum AppState {
    Config,
    Connecting,
    Monitoring,
}

#[derive(Clone, Debug, PartialEq)]
enum ConfigField {
    Host,
    Username,
    Password,
    UseSSHKey,
    SSHKeyPath,
}

struct ConfigScreen {
    host: String,
    username: String,
    password: String,
    use_ssh_key: bool,
    ssh_key_path: String,
    current_field: ConfigField,
    error_message: Option<String>,
}

struct LoadingScreen {
    progress: u16,
    direction: i16,
    message: String,
}

#[derive(Clone, Debug, PartialEq)]
enum SortBy {
    Cpu,
    Ram,
}

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
    state: AppState,
    config: ConfigScreen,
    loading: LoadingScreen,
    users: Vec<UserStats>,
    history: Vec<HistoricalData>,
    selected_user: usize,
    sort_by: SortBy,
    should_quit: bool,
}

impl ConfigScreen {
    fn new() -> Self {
        ConfigScreen {
            host: String::new(),
            username: String::new(),
            password: String::new(),
            use_ssh_key: false,
            ssh_key_path: format!("{}/.ssh/id_rsa", std::env::var("HOME").unwrap_or_default()),
            current_field: ConfigField::Host,
            error_message: None,
        }
    }

    fn next_field(&mut self) {
        self.current_field = match self.current_field {
            ConfigField::Host => ConfigField::Username,
            ConfigField::Username => ConfigField::UseSSHKey,
            ConfigField::UseSSHKey => {
                if self.use_ssh_key {
                    ConfigField::SSHKeyPath
                } else {
                    ConfigField::Password
                }
            }
            ConfigField::Password => ConfigField::Host,
            ConfigField::SSHKeyPath => ConfigField::Host,
        };
    }

    fn previous_field(&mut self) {
        self.current_field = match self.current_field {
            ConfigField::Host => {
                if self.use_ssh_key {
                    ConfigField::SSHKeyPath
                } else {
                    ConfigField::Password
                }
            }
            ConfigField::Username => ConfigField::Host,
            ConfigField::UseSSHKey => ConfigField::Username,
            ConfigField::Password => ConfigField::UseSSHKey,
            ConfigField::SSHKeyPath => ConfigField::UseSSHKey,
        };
    }

    fn handle_char(&mut self, c: char) {
        match self.current_field {
            ConfigField::Host => self.host.push(c),
            ConfigField::Username => self.username.push(c),
            ConfigField::Password => {
                if !self.use_ssh_key {
                    self.password.push(c)
                }
            }
            ConfigField::SSHKeyPath => {
                if self.use_ssh_key {
                    self.ssh_key_path.push(c)
                }
            }
            ConfigField::UseSSHKey => {}
        }
    }

    fn handle_backspace(&mut self) {
        match self.current_field {
            ConfigField::Host => {
                self.host.pop();
            }
            ConfigField::Username => {
                self.username.pop();
            }
            ConfigField::Password => {
                if !self.use_ssh_key {
                    self.password.pop();
                }
            }
            ConfigField::SSHKeyPath => {
                if self.use_ssh_key {
                    self.ssh_key_path.pop();
                }
            }
            ConfigField::UseSSHKey => {}
        }
    }

    fn toggle_ssh_key(&mut self) {
        if self.current_field == ConfigField::UseSSHKey {
            self.use_ssh_key = !self.use_ssh_key;
            if self.use_ssh_key {
                self.password.clear();
            }
        }
    }

    fn is_valid(&self) -> bool {
        !self.host.is_empty() 
            && !self.username.is_empty() 
            && (self.use_ssh_key || !self.password.is_empty())
    }
}

impl LoadingScreen {
    fn new() -> Self {
        LoadingScreen {
            progress: 0,
            direction: 1,
            message: "Connecting to SSH server...".to_string(),
        }
    }

    fn update(&mut self) {
        if self.direction > 0 {
            self.progress += 2;
            if self.progress >= 100 {
                self.direction = -1;
            }
        } else {
            if self.progress <= 2 {
                self.direction = 1;
                self.progress = 0;
            } else {
                self.progress -= 2;
            }
        }
    }
}

impl App {
    fn new() -> App {
        App {
            state: AppState::Config,
            config: ConfigScreen::new(),
            loading: LoadingScreen::new(),
            users: Vec::new(),
            history: Vec::new(),
            selected_user: 0,
            sort_by: SortBy::Cpu,
            should_quit: false,
        }
    }

    fn update_data(&mut self, users: Vec<UserStats>) {
        self.users = users;
        self.sort_users();
        
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

    fn sort_users(&mut self) {
        match self.sort_by {
            SortBy::Cpu => {
                self.users.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap());
            }
            SortBy::Ram => {
                self.users.sort_by(|a, b| b.ram_mb.partial_cmp(&a.ram_mb).unwrap());
            }
        }
    }

    fn set_sort(&mut self, sort_by: SortBy) {
        self.sort_by = sort_by;
        self.sort_users();
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

fn ssh_get_user_stats(
    host: &str,
    user: &str,
    password: Option<&str>,
    ssh_key_path: Option<&str>,
) -> Result<Vec<UserStats>> {
    let tcp = TcpStream::connect(format!("{}:22", host))?;
    let mut sess = Session::new()?;
    sess.set_tcp_stream(tcp);
    sess.handshake()?;

    // Authenticate using either password or SSH key
    if let Some(key_path) = ssh_key_path {
        sess.userauth_pubkey_file(user, None, std::path::Path::new(key_path), None)?;
    } else if let Some(pwd) = password {
        sess.userauth_password(user, pwd)?;
    } else {
        return Err(anyhow::anyhow!("No authentication method provided"));
    }

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

fn ui(f: &mut Frame, app: &App) {
    match app.state {
        AppState::Config => render_config_screen(f, &app.config),
        AppState::Connecting => render_loading_screen(f, &app.loading),
        AppState::Monitoring => render_monitoring_screen(f, app),
    }
}

fn render_config_screen(f: &mut Frame, config: &ConfigScreen) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(2),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("SSH Server Monitor - Configuration")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Host
    let host_style = if config.current_field == ConfigField::Host {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let host = Paragraph::new(format!("Host: {}", config.host))
        .style(host_style)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(host, chunks[1]);

    // Username
    let username_style = if config.current_field == ConfigField::Username {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let username = Paragraph::new(format!("Username: {}", config.username))
        .style(username_style)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(username, chunks[2]);

    // Use SSH Key checkbox
    let ssh_key_style = if config.current_field == ConfigField::UseSSHKey {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let checkbox = if config.use_ssh_key { "[X]" } else { "[ ]" };
    let use_ssh_key = Paragraph::new(format!("{} Use SSH Key (Space to toggle)", checkbox))
        .style(ssh_key_style)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(use_ssh_key, chunks[3]);

    // Password or SSH Key Path
    if config.use_ssh_key {
        let key_path_style = if config.current_field == ConfigField::SSHKeyPath {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let key_path = Paragraph::new(format!("SSH Key Path: {}", config.ssh_key_path))
            .style(key_path_style)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(key_path, chunks[4]);
    } else {
        let password_style = if config.current_field == ConfigField::Password {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let password_display = "*".repeat(config.password.len());
        let password = Paragraph::new(format!("Password: {}", password_display))
            .style(password_style)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(password, chunks[4]);
    }

    // Instructions
    let instructions = vec![
        Line::from(vec![
            Span::styled("Tab/Shift+Tab", Style::default().fg(Color::Green)),
            Span::raw(": Navigate fields"),
        ]),
        Line::from(vec![
            Span::styled("Space", Style::default().fg(Color::Green)),
            Span::raw(": Toggle SSH Key"),
        ]),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::raw(": Connect"),
        ]),
        Line::from(vec![
            Span::styled("Esc/q", Style::default().fg(Color::Green)),
            Span::raw(": Quit"),
        ]),
    ];
    let help = Paragraph::new(instructions)
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    f.render_widget(help, chunks[6]);

    // Status/Error message
    let status_text = if let Some(ref error) = config.error_message {
        vec![Line::from(Span::styled(
            format!("Error: {}", error),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))]
    } else if config.is_valid() {
        vec![Line::from(Span::styled(
            "Press Enter to connect",
            Style::default().fg(Color::Green),
        ))]
    } else {
        vec![Line::from(Span::styled(
            "Fill in all required fields",
            Style::default().fg(Color::Yellow),
        ))]
    };
    let status = Paragraph::new(status_text)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(status, chunks[7]);
}

fn render_loading_screen(f: &mut Frame, loading: &LoadingScreen) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(4)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Percentage(40),
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("SSH Server Monitor")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    // Message
    let message = Paragraph::new(loading.message.clone())
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(message, chunks[1]);

    // Progress bar
    let progress_width = chunks[2].width.saturating_sub(4) as u16;
    let bar_position = ((loading.progress as f64 / 100.0) * progress_width as f64) as u16;
    
    let bar_char = "█";
    let empty_char = "░";
    
    let mut bar_string = String::new();
    for i in 0..progress_width {
        if i >= bar_position.saturating_sub(5) && i <= bar_position {
            bar_string.push_str(bar_char);
        } else {
            bar_string.push_str(empty_char);
        }
    }
    
    let progress_bar = Paragraph::new(bar_string)
        .style(Style::default().fg(Color::Green))
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title(format!("Progress")));
    f.render_widget(progress_bar, chunks[2]);

    // Hint
    let hint = Paragraph::new("Press Esc to cancel")
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(hint, chunks[3]);
}

fn render_monitoring_screen(f: &mut Frame, app: &App) {
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
    let cpu_header = if app.sort_by == SortBy::Cpu {
        format!("CPU % ▼")
    } else {
        "CPU %".to_string()
    };
    let ram_header = if app.sort_by == SortBy::Ram {
        format!("RAM (MB) ▼")
    } else {
        "RAM (MB)".to_string()
    };
    
    let header = Row::new(vec!["User", &cpu_header, &ram_header, "Last Updated"])
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
        Line::from("c: Sort by CPU"),
        Line::from("r: Sort by RAM"),
        Line::from("q/Esc: Back"),
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
                        Line::from("0"),
                        Line::from(format!("{:.0}", max_cpu * 0.5)),
                        Line::from(format!("{:.0}", max_cpu)),
                    ]),
            );        f.render_widget(cpu_chart, graph_chunks[0]);
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
                        Line::from("0"),
                        Line::from(format!("{:.0}", max_ram * 0.5)),
                        Line::from(format!("{:.0}", max_ram)),
                    ]),
            );        f.render_widget(ram_chart, graph_chunks[1]);
    }
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: Arc<Mutex<App>>) -> Result<()> {
    let mut data_thread: Option<std::thread::JoinHandle<()>> = None;

    loop {
        {
            let mut app_guard = app.lock().unwrap();
            
            // Update loading animation
            if app_guard.state == AppState::Connecting {
                app_guard.loading.update();
            }
            
            terminal.draw(|f| ui(f, &app_guard))?;

            if app_guard.should_quit {
                break;
            }
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                let mut app_guard = app.lock().unwrap();

                match app_guard.state {
                    AppState::Config => {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => app_guard.should_quit = true,
                            KeyCode::Tab => {
                                if key.modifiers.contains(KeyModifiers::SHIFT) {
                                    app_guard.config.previous_field();
                                } else {
                                    app_guard.config.next_field();
                                }
                            }
                            KeyCode::Up => app_guard.config.previous_field(),
                            KeyCode::Down => app_guard.config.next_field(),
                            KeyCode::Char(' ') => app_guard.config.toggle_ssh_key(),
                            KeyCode::Char(c) => app_guard.config.handle_char(c),
                            KeyCode::Backspace => app_guard.config.handle_backspace(),
                            KeyCode::Enter => {
                                if app_guard.config.is_valid() {
                                    // Switch to loading state
                                    app_guard.state = AppState::Connecting;
                                    app_guard.loading = LoadingScreen::new();
                                    
                                    let host = app_guard.config.host.clone();
                                    let user = app_guard.config.username.clone();
                                    let password = if app_guard.config.use_ssh_key {
                                        None
                                    } else {
                                        Some(app_guard.config.password.clone())
                                    };
                                    let ssh_key = if app_guard.config.use_ssh_key {
                                        Some(app_guard.config.ssh_key_path.clone())
                                    } else {
                                        None
                                    };

                                    // Try to connect in a background thread
                                    let app_clone = app.clone();
                                    std::thread::spawn(move || {
                                        match ssh_get_user_stats(
                                            &host,
                                            &user,
                                            password.as_deref(),
                                            ssh_key.as_deref(),
                                        ) {
                                            Ok(users) => {
                                                let mut app_guard = app_clone.lock().unwrap();
                                                app_guard.update_data(users);
                                                app_guard.state = AppState::Monitoring;
                                                app_guard.config.error_message = None;

                                                // Start data collection thread
                                                let app_clone2 = app_clone.clone();
                                                let host_clone = host.clone();
                                                let user_clone = user.clone();
                                                let password_clone = password.clone();
                                                let ssh_key_clone = ssh_key.clone();

                                                std::thread::spawn(move || loop {
                                                    std::thread::sleep(Duration::from_secs(2));
                                                    match ssh_get_user_stats(
                                                        &host_clone,
                                                        &user_clone,
                                                        password_clone.as_deref(),
                                                        ssh_key_clone.as_deref(),
                                                    ) {
                                                        Ok(users) => {
                                                            let mut app = app_clone2.lock().unwrap();
                                                            if app.state == AppState::Monitoring {
                                                                app.update_data(users);
                                                            } else {
                                                                break;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            eprintln!("Error fetching stats: {}", e);
                                                        }
                                                    }
                                                });
                                            }
                                            Err(e) => {
                                                let mut app_guard = app_clone.lock().unwrap();
                                                app_guard.state = AppState::Config;
                                                app_guard.config.error_message =
                                                    Some(format!("Connection failed: {}", e));
                                            }
                                        }
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                    AppState::Connecting => match key.code {
                        KeyCode::Esc => {
                            app_guard.state = AppState::Config;
                        }
                        _ => {}
                    },
                    AppState::Monitoring => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app_guard.state = AppState::Config;
                            app_guard.users.clear();
                            app_guard.history.clear();
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            app_guard.set_sort(SortBy::Cpu);
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            app_guard.set_sort(SortBy::Ram);
                        }
                        KeyCode::Down => app_guard.next_user(),
                        KeyCode::Up => app_guard.previous_user(),
                        _ => {}
                    },
                }
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Arc::new(Mutex::new(App::new()));
    let res = run_app(&mut terminal, app);

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

