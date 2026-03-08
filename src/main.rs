mod app;
mod utils;
mod view;

use anyhow::Result;
use app::App;
use crossterm::{
  event::{self, Event, KeyEventKind},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{
  Terminal,
  backend::{Backend, CrosstermBackend},
};
use std::io;

fn main() -> Result<()> {
  // Enable raw mode for terminal UI
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  // Enter alternate screen (mouse capture disabled to allow terminal's native selection)
  execute!(stdout, EnterAlternateScreen)?;
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend)?;

  // Create app state
  let mut app = App::new();

  // Run the main app loop
  let result = run_app(&mut terminal, &mut app);

  // Restore terminal settings
  disable_raw_mode()?;
  execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
  terminal.show_cursor()?;

  result
}

/// Run the main application loop
fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()>
where
  B::Error: Send + Sync + 'static,
{
  loop {
    // Draw the UI
    terminal.draw(|f| app.draw(f))?;

    // Handle events
    match event::read()? {
      Event::Key(key) => {
        // Only handle key press events to avoid duplicate processing
        if key.kind == KeyEventKind::Press {
          app.handle_key(key);
        }
      }
      // Mouse events are disabled to allow terminal's native selection
      // Event::Mouse(_) => {}
      _ => {}
    }

    // Check if we should exit
    if app.data.should_exit {
      return Ok(());
    }
  }
}
