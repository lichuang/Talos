//! Event stream plumbing for the TUI.
//!
//! - [`EventBroker`] holds the shared crossterm stream so multiple callers reuse the same
//!   input source and can drop/recreate it on pause/resume without rebuilding consumers.
//! - [`TuiEventStream`] wraps a draw event subscription plus the shared [`EventBroker`] and maps crossterm
//!   events into [`TuiEvent`].
//! - [`EventSource`] abstracts the underlying event producer; the real implementation is
//!   [`CrosstermEventSource`] and tests can swap in [`FakeEventSource`].
//!
//! The motivation for dropping/recreating the crossterm event stream is to enable the TUI to fully relinquish stdin.
//! If the stream is not dropped, it will continue to read from stdin even if it is not actively being polled
//! (due to how crossterm's EventStream is implemented), potentially stealing input from other processes reading stdin,
//! like terminal text editors. This race can cause missed input or capturing terminal query responses (for example, OSC palette/size queries)
//! that the other process expects to read. Stopping polling, instead of dropping the stream, is only sufficient when the
//! pause happens before the stream enters a pending state; otherwise the crossterm reader thread may keep reading
//! from stdin, so the safer approach is to drop and recreate the event stream when we need to hand off the terminal.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;

use crossterm::event::Event;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tokio_stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::WatchStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use super::TuiEvent;

/// Result type produced by an event source.
pub type EventResult = std::io::Result<Event>;

/// Abstraction over a source of terminal events. Allows swapping in a fake for tests.
/// Value in production is [`CrosstermEventSource`].
pub trait EventSource: Send + 'static {
  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<EventResult>>;
}

/// Shared crossterm input state for all [`TuiEventStream`] instances. A single crossterm EventStream
/// is reused so all streams still see the same input source.
///
/// This intermediate layer enables dropping/recreating the underlying EventStream (pause/resume) without rebuilding consumers.
pub struct EventBroker<S: EventSource = CrosstermEventSource> {
  state: Mutex<EventBrokerState<S>>,
  resume_events_tx: watch::Sender<()>,
}

/// Tracks state of underlying [`EventSource`].
#[allow(dead_code)]
enum EventBrokerState<S: EventSource> {
  Paused,     // Underlying event source (i.e., crossterm EventStream) dropped
  Start,      // A new event source will be created on next poll
  Running(S), // Event source is currently running
}

impl<S: EventSource + Default> EventBrokerState<S> {
  /// Return the running event source, starting it if needed; None when paused.
  fn active_event_source_mut(&mut self) -> Option<&mut S> {
    match self {
      EventBrokerState::Paused => None,
      EventBrokerState::Start => {
        *self = EventBrokerState::Running(S::default());
        match self {
          EventBrokerState::Running(events) => Some(events),
          EventBrokerState::Paused | EventBrokerState::Start => unreachable!(),
        }
      }
      EventBrokerState::Running(events) => Some(events),
    }
  }
}

impl<S: EventSource + Default> EventBroker<S> {
  pub fn new() -> Self {
    let (resume_events_tx, _resume_events_rx) = watch::channel(());
    Self {
      state: Mutex::new(EventBrokerState::Start),
      resume_events_tx,
    }
  }

  /// Drop the underlying event source
  #[allow(dead_code)]
  pub fn pause_events(&self) {
    let mut state = self
      .state
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    *state = EventBrokerState::Paused;
  }

  /// Create a new instance of the underlying event source
  #[allow(dead_code)]
  pub fn resume_events(&self) {
    let mut state = self
      .state
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    *state = EventBrokerState::Start;
    let _ = self.resume_events_tx.send(());
  }

  /// Subscribe to a notification that fires whenever [`Self::resume_events`] is called.
  pub fn resume_events_rx(&self) -> watch::Receiver<()> {
    self.resume_events_tx.subscribe()
  }
}

impl<S: EventSource + Default> Default for EventBroker<S> {
  fn default() -> Self {
    Self::new()
  }
}

/// Real crossterm-backed event source.
pub struct CrosstermEventSource(pub crossterm::event::EventStream);

impl Default for CrosstermEventSource {
  fn default() -> Self {
    Self(crossterm::event::EventStream::new())
  }
}

impl EventSource for CrosstermEventSource {
  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<EventResult>> {
    Pin::new(&mut self.get_mut().0).poll_next(cx)
  }
}

/// TuiEventStream is a struct for reading TUI events (draws and user input).
/// Each instance has its own draw subscription (the draw channel is broadcast, so
/// multiple receivers are fine), while crossterm input is funneled through a
/// single shared [`EventBroker`] because crossterm uses a global stdin reader and
/// does not support fan-out. Multiple TuiEventStream instances can exist during the app lifetime
/// (for nested or sequential screens), but only one should be polled at a time,
/// otherwise one instance can consume ("steal") input events and the other will miss them.
pub struct TuiEventStream<S: EventSource + Default + Unpin = CrosstermEventSource> {
  broker: Arc<EventBroker<S>>,
  draw_stream: BroadcastStream<()>,
  resume_stream: WatchStream<()>,
  terminal_focused: Arc<AtomicBool>,
  poll_draw_first: bool,
}

impl<S: EventSource + Default + Unpin> TuiEventStream<S> {
  pub fn new(
    broker: Arc<EventBroker<S>>,
    draw_rx: broadcast::Receiver<()>,
    terminal_focused: Arc<AtomicBool>,
  ) -> Self {
    let resume_stream = WatchStream::from_changes(broker.resume_events_rx());
    Self {
      broker,
      draw_stream: BroadcastStream::new(draw_rx),
      resume_stream,
      terminal_focused,
      poll_draw_first: false,
    }
  }

  /// Poll the shared crossterm stream for the next mapped `TuiEvent`.
  pub fn poll_crossterm_event(&mut self, cx: &mut Context<'_>) -> Poll<Option<TuiEvent>> {
    // Some crossterm events map to None (e.g. FocusLost, mouse); loop so we keep polling
    // until we return a mapped event, hit Pending, or see EOF/error.
    loop {
      let poll_result = {
        let mut state = self
          .broker
          .state
          .lock()
          .unwrap_or_else(std::sync::PoisonError::into_inner);
        let events = match state.active_event_source_mut() {
          Some(events) => events,
          None => {
            drop(state);
            // Poll resume_stream so resume_events wakes a stream paused here
            match Pin::new(&mut self.resume_stream).poll_next(cx) {
              Poll::Ready(Some(())) => continue,
              Poll::Ready(None) => return Poll::Ready(None),
              Poll::Pending => return Poll::Pending,
            }
          }
        };
        match Pin::new(events).poll_next(cx) {
          Poll::Ready(Some(Ok(event))) => Some(event),
          Poll::Ready(Some(Err(_))) | Poll::Ready(None) => {
            *state = EventBrokerState::Start;
            return Poll::Ready(None);
          }
          Poll::Pending => {
            drop(state);
            // Poll resume_stream so resume_events can wake us even while waiting on stdin
            match Pin::new(&mut self.resume_stream).poll_next(cx) {
              Poll::Ready(Some(())) => continue,
              Poll::Ready(None) => return Poll::Ready(None),
              Poll::Pending => return Poll::Pending,
            }
          }
        }
      };

      if let Some(mapped) = poll_result.and_then(|event| self.map_crossterm_event(event)) {
        return Poll::Ready(Some(mapped));
      }
    }
  }

  /// Poll the draw broadcast stream for the next draw event.
  pub fn poll_draw_event(&mut self, cx: &mut Context<'_>) -> Poll<Option<TuiEvent>> {
    match Pin::new(&mut self.draw_stream).poll_next(cx) {
      Poll::Ready(Some(Ok(()))) => Poll::Ready(Some(TuiEvent::Draw)),
      Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(_)))) => {
        Poll::Ready(Some(TuiEvent::Draw))
      }
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Pending => Poll::Pending,
    }
  }

  /// Map a crossterm event to a [`TuiEvent`], skipping events we don't use.
  fn map_crossterm_event(&mut self, event: Event) -> Option<TuiEvent> {
    match event {
      Event::Key(key_event) => Some(TuiEvent::Key(key_event)),
      Event::Resize(_, _) => Some(TuiEvent::Draw),
      Event::Paste(pasted) => Some(TuiEvent::Paste(pasted)),
      Event::FocusGained => {
        self.terminal_focused.store(true, Ordering::Relaxed);
        Some(TuiEvent::Draw)
      }
      Event::FocusLost => {
        self.terminal_focused.store(false, Ordering::Relaxed);
        None
      }
      _ => None,
    }
  }
}

impl<S: EventSource + Default + Unpin> Unpin for TuiEventStream<S> {}

impl<S: EventSource + Default + Unpin> Stream for TuiEventStream<S> {
  type Item = TuiEvent;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    // approximate fairness + no starvation via round-robin.
    let draw_first = self.poll_draw_first;
    self.poll_draw_first = !self.poll_draw_first;

    if draw_first {
      if let Poll::Ready(event) = self.poll_draw_event(cx) {
        return Poll::Ready(event);
      }
      if let Poll::Ready(event) = self.poll_crossterm_event(cx) {
        return Poll::Ready(event);
      }
    } else {
      if let Poll::Ready(event) = self.poll_crossterm_event(cx) {
        return Poll::Ready(event);
      }
      if let Poll::Ready(event) = self.poll_draw_event(cx) {
        return Poll::Ready(event);
      }
    }

    Poll::Pending
  }
}
