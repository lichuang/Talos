use crossterm::event::KeyEvent;
use ratatui::Frame;

use crate::app::AppData;

pub mod chat;
pub mod home;

pub use chat::ChatView;
pub use home::HomeView;

/// Trait for all views in the application
pub trait View {
  /// Handle keyboard events
  ///
  /// # Arguments
  /// * `data` - The application data that can be modified
  /// * `key` - The key code that was pressed
  ///
  /// # Returns
  /// * `Some(Box<dyn View>)` - If the view wants to switch to a new view
  /// * `None` - If no view switch is needed
  fn handle_key(&mut self, data: &mut AppData, key: KeyEvent) -> Option<Box<dyn View>>;

  /// Draw the view on the frame
  ///
  /// # Arguments
  /// * `f` - The frame to draw on
  /// * `data` - The application data (for accessing messages, etc.)
  fn draw(&self, f: &mut Frame, data: &AppData);
}
