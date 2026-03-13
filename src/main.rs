mod app;
mod config;
mod process;
mod ui;

use app::App;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "sspm", about = "Stupid Simple Process Manager")]
struct Cli {
    /// Profile to use (defaults to "default")
    profile: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let profile = cli.profile.as_deref().unwrap_or("default");

    let config = config::Config::load("sspm.toml")?;
    let mut app = App::new(&config, profile)?;

    // Start processes that are checked by the profile
    app.start_checked();

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app).await;

    // Cleanup: stop all processes, restore terminal
    app.stop_all().await;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        // Drain output from child processes
        app.drain_output();

        // Draw UI
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for events with a short timeout (~16ms for ~60fps)
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) => {
                        app.should_quit = true;
                    }
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    (KeyCode::Up | KeyCode::Char('k'), _) => {
                        app.move_up();
                    }
                    (KeyCode::Down | KeyCode::Char('j'), _) => {
                        app.move_down();
                    }
                    (KeyCode::Char(' ') | KeyCode::Enter, _) => {
                        app.toggle_selected().await;
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
