use crate::view::{HomeView, View};

/// Application data that can be modified by views
pub struct AppData {
  /// Whether the app should exit
  pub should_exit: bool,
  /// Message history (for chat)
  pub messages: Vec<String>,
}

impl AppData {
  /// Create a new app data instance
  pub fn new() -> Self {
    Self {
      should_exit: false,
      messages: Vec::new(),
    }
  }
}

impl Default for AppData {
  fn default() -> Self {
    Self::new()
  }
}

/// Application state
pub struct App {
  /// Application data
  pub data: AppData,
  /// Current view (dynamic dispatch)
  pub view: Box<dyn View>,
}

impl App {
  /// Create a new app instance
  pub fn new() -> Self {
    Self {
      data: AppData::new(),
      view: Box::new(HomeView::new()),
    }
  }

  /// Handle keyboard events
  pub fn handle_key(&mut self, key: crossterm::event::KeyCode) {
    // Now we can borrow view and data separately
    if let Some(new_view) = self.view.handle_key(&mut self.data, key) {
      self.view = new_view;
    }
  }

  /// Draw the current view
  pub fn draw(&self, f: &mut ratatui::Frame) {
    self.view.draw(f, &self.data);
  }
}

impl Default for App {
  fn default() -> Self {
    Self::new()
  }
}
