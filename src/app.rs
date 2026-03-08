use crate::tui::FrameRequester;
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
  /// Frame requester for animation scheduling
  frame_requester: Option<FrameRequester>,
}

impl App {
  /// Create a new app instance
  pub fn new() -> Self {
    Self {
      data: AppData::new(),
      view: Box::new(HomeView::new()),
      frame_requester: None,
    }
  }

  /// Handle keyboard events
  pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
    if let Some(new_view) = self.view.handle_key(&mut self.data, key) {
      self.view = new_view;
      // Re-set frame requester when view changes
      if let Some(ref frame_requester) = self.frame_requester {
        self.view.set_frame_requester(frame_requester.clone());
      }
    }
  }

  /// Draw the current view
  pub fn draw(&self, f: &mut ratatui::Frame) {
    self.view.draw(f, &self.data);
  }

  /// Called when a new frame is about to be rendered
  pub fn on_frame(&mut self, frame_requester: &FrameRequester) {
    self.view.on_frame(frame_requester);
  }

  /// Set the frame requester for the current view
  pub fn set_frame_requester(&mut self, frame_requester: FrameRequester) {
    self.frame_requester = Some(frame_requester.clone());
    self.view.set_frame_requester(frame_requester);
  }
}

impl Default for App {
  fn default() -> Self {
    Self::new()
  }
}
