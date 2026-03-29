//! Test helpers for nightshift-core consumers.
//!
//! Exposed under `#[cfg(any(test, feature = "testing"))]` so external crates
//! can build `PinnedPrompt<Vec<u8>, MockBackend>` in their own test suites
//! without duplicating the struct or the crossterm boilerplate.

use std::collections::VecDeque;
use std::io;

use crossterm::event::Event;

use crate::pinned_prompt::{PinnedPrompt, TerminalBackend};

/// A canned-event terminal backend for tests.
///
/// Returns pre-supplied events from a queue and reports a fixed terminal size.
/// All raw-mode operations are no-ops.
pub struct MockBackend {
    events: VecDeque<Event>,
    width: u16,
    height: u16,
}

impl MockBackend {
    /// Create a backend with the given dimensions and event queue.
    pub fn new(width: u16, height: u16, events: Vec<Event>) -> Self {
        MockBackend {
            events: events.into(),
            width,
            height,
        }
    }
}

impl TerminalBackend for MockBackend {
    fn read_event(&mut self) -> io::Result<Event> {
        self.events
            .pop_front()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no more events"))
    }

    fn size(&self) -> io::Result<(u16, u16)> {
        Ok((self.width, self.height))
    }

    fn enable_raw_mode(&self) -> io::Result<()> {
        Ok(())
    }

    fn disable_raw_mode(&self) -> io::Result<()> {
        Ok(())
    }
}

/// Convenience constructor: returns a `PinnedPrompt<Vec<u8>, MockBackend>`
/// with the given terminal dimensions and canned events.
pub fn make_prompt(
    width: u16,
    height: u16,
    events: Vec<Event>,
) -> PinnedPrompt<Vec<u8>, MockBackend> {
    PinnedPrompt::with_backend(Vec::new(), MockBackend::new(width, height, events))
        .expect("mock prompt should initialize")
}
