//! Time-related utilities.

use std::time::Duration;

/// A very long duration used as a fallback timeout.
/// Approximately one year in seconds.
pub const ONE_YEAR: Duration = Duration::from_secs(60 * 60 * 24 * 365);
