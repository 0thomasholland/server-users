mod ssh;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use ui::{App, AppState};



fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: Arc<Mutex<App>>) -> Result<()> {
    let _data_thread: Option<std::thread::JoinHandle<()>> = None;

    loop {
        {
            let mut app_guard = app.lock().unwrap();
            
            // Update loading animation
            if app_guard.state == AppState::Connecting {
                app_guard.loading.update();
            }
            
            terminal.draw(|f| ui::ui(f, &app_guard))?;

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
                                    app_guard.loading = ui::LoadingScreen::new();
                                    
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
                                        match ssh::get_user_stats(
                                            &host,
                                            &user,
                                            password.as_deref(),
                                            ssh_key.as_deref(),
                                        ) {
                                            Ok((users, total_ram)) => {
                                                let mut app_guard = app_clone.lock().unwrap();
                                                app_guard.total_ram_mb = total_ram;
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
                                                    match ssh::get_user_stats(
                                                        &host_clone,
                                                        &user_clone,
                                                        password_clone.as_deref(),
                                                        ssh_key_clone.as_deref(),
                                                    ) {
                                                        Ok((users, total_ram)) => {
                                                            let mut app = app_clone2.lock().unwrap();
                                                            if app.state == AppState::Monitoring {
                                                                app.total_ram_mb = total_ram;
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
                            app_guard.set_sort(ui::SortBy::Cpu);
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            app_guard.set_sort(ui::SortBy::Ram);
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

