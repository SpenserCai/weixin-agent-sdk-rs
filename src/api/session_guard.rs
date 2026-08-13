//! In-memory poll-loop pause/cooldown guard.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, Result};
use crate::types::SESSION_PAUSE_DURATION_MS;

/// Guards the long-poll loop during a stale-token cooldown period.
///
/// Only the poll loop consults this guard; outbound calls are not blocked.
pub(crate) struct SessionGuard {
    pause_until_ms: AtomicU64,
}

impl SessionGuard {
    /// Create a new (unpaused) guard.
    pub fn new() -> Self {
        Self {
            pause_until_ms: AtomicU64::new(0),
        }
    }

    /// Pause the poll loop for one hour from now.
    pub fn pause(&self) {
        let until = now_ms() + SESSION_PAUSE_DURATION_MS;
        self.pause_until_ms.store(until, Ordering::Relaxed);
        tracing::info!(until_ms = until, "poll loop paused");
    }

    /// Returns `true` if currently within the cooldown window.
    pub fn is_paused(&self) -> bool {
        let until = self.pause_until_ms.load(Ordering::Relaxed);
        until > 0 && now_ms() < until
    }

    /// Returns remaining pause time in milliseconds (0 if not paused).
    pub fn remaining_ms(&self) -> u64 {
        let until = self.pause_until_ms.load(Ordering::Relaxed);
        if until == 0 {
            return 0;
        }
        until.saturating_sub(now_ms())
    }

    /// Returns `Ok(())` if active, or [`Error::TokenStale`] if paused.
    #[allow(dead_code)] // Reserved for callers that want to gate work on the cooldown window.
    pub fn assert_active(&self) -> Result<()> {
        if self.is_paused() {
            Err(Error::TokenStale)
        } else {
            Ok(())
        }
    }
}

impl Default for SessionGuard {
    fn default() -> Self {
        Self::new()
    }
}

use crate::util::now_ms;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_not_paused() {
        let guard = SessionGuard::new();
        assert!(!guard.is_paused());
        assert_eq!(guard.remaining_ms(), 0);
        assert!(guard.assert_active().is_ok());
    }

    #[test]
    fn pause_and_check() {
        let guard = SessionGuard::new();
        guard.pause();
        assert!(guard.is_paused());
        assert!(guard.remaining_ms() > 0);
    }

    #[test]
    fn assert_active_when_paused() {
        let guard = SessionGuard::new();
        guard.pause();
        assert!(guard.assert_active().is_err());
    }

    #[test]
    fn assert_active_returns_token_stale_when_paused() {
        let guard = SessionGuard::new();
        guard.pause();
        assert!(matches!(guard.assert_active(), Err(Error::TokenStale)));
    }
}
