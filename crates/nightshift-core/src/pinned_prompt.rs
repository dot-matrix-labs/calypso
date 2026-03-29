//! Pinned-prompt UI for step-through execution mode.
//!
//! Renders a fixed prompt on the last terminal line while log output scrolls
//! above it using ANSI scroll regions (DECSTBM).

use std::io::{self, Write};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;

/// Result of reading a confirmation keypress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Yes,
    No,
    Quit,
}

/// Abstraction over event reading and terminal queries, allowing tests to
/// inject canned events without a real terminal.
pub trait TerminalBackend {
    /// Read the next terminal event (blocking).
    fn read_event(&mut self) -> io::Result<Event>;

    /// Query the current terminal size.
    fn size(&self) -> io::Result<(u16, u16)>;

    /// Enable raw mode.
    fn enable_raw_mode(&self) -> io::Result<()>;

    /// Disable raw mode.
    fn disable_raw_mode(&self) -> io::Result<()>;
}

/// Real terminal backend using crossterm.
pub struct CrosstermBackend;

impl TerminalBackend for CrosstermBackend {
    fn read_event(&mut self) -> io::Result<Event> {
        crossterm::event::read()
    }

    fn size(&self) -> io::Result<(u16, u16)> {
        terminal::size()
    }

    fn enable_raw_mode(&self) -> io::Result<()> {
        terminal::enable_raw_mode()
    }

    fn disable_raw_mode(&self) -> io::Result<()> {
        terminal::disable_raw_mode()
    }
}

/// A pinned prompt that stays on the last terminal line while log output
/// scrolls above it.
pub struct PinnedPrompt<W: Write, B: TerminalBackend> {
    writer: W,
    backend: B,
    height: u16,
    width: u16,
}

impl PinnedPrompt<io::Stdout, CrosstermBackend> {
    /// Create a new pinned prompt attached to stdout with a real terminal.
    pub fn new() -> io::Result<Self> {
        Self::with_backend(io::stdout(), CrosstermBackend)
    }
}

impl<W: Write, B: TerminalBackend> PinnedPrompt<W, B> {
    /// Create a pinned prompt with an injected writer and backend.
    pub fn with_backend(writer: W, backend: B) -> io::Result<Self> {
        let (width, height) = backend.size()?;
        backend.enable_raw_mode()?;

        let mut prompt = PinnedPrompt {
            writer,
            backend,
            height,
            width,
        };
        prompt.set_scroll_region()?;
        prompt.clear_prompt_line()?;
        Ok(prompt)
    }

    /// Set the scroll region to all rows except the last.
    fn set_scroll_region(&mut self) -> io::Result<()> {
        if self.height < 2 {
            return Ok(());
        }
        // DECSTBM: set scroll region to rows 1..(height-1) (1-based)
        write!(self.writer, "\x1b[1;{}r", self.height - 1)?;
        // Move cursor into the scroll region
        write!(self.writer, "\x1b[{};1H", self.height - 1)?;
        self.writer.flush()
    }

    /// Clear the prompt (bottom) line.
    fn clear_prompt_line(&mut self) -> io::Result<()> {
        write!(self.writer, "\x1b[{};1H\x1b[2K", self.height)?;
        self.writer.flush()
    }

    /// Return a reference to the written bytes when `W` is `Vec<u8>`.
    ///
    /// Used in tests to inspect what was written to the prompt.
    pub fn writer_ref(&self) -> &W {
        &self.writer
    }

    /// Return a mutable reference to the writer so tests can clear it between
    /// assertions.
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Re-apply the scroll region after subprocess output may have disrupted it.
    ///
    /// Call this after any `driver.step()` invocation and before the next
    /// `log()` or `show_prompt()` call to ensure ANSI scroll-region state is
    /// intact.
    pub fn repair_scroll_region(&mut self) -> io::Result<()> {
        self.set_scroll_region()
    }

    /// Write a log line above the prompt (within the scroll region).
    pub fn log(&mut self, line: &str) -> io::Result<()> {
        write!(self.writer, "\x1b[{};1H", self.height - 1)?;
        write!(self.writer, "\r\n{line}")?;
        self.writer.flush()
    }

    /// Update and display the prompt text on the pinned bottom line.
    pub fn show_prompt(&mut self, text: &str) -> io::Result<()> {
        let display: String = text.chars().take(self.width as usize).collect();
        write!(self.writer, "\x1b[{};1H\x1b[2K{display}", self.height)?;
        self.writer.flush()
    }

    /// Read a single confirmation keypress (Y/Enter = Yes, n = No, q = Quit).
    pub fn read_confirmation(&mut self) -> io::Result<Confirmation> {
        loop {
            let event = self.backend.read_event()?;
            match event {
                Event::Key(KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }) => return Ok(Confirmation::Quit),

                Event::Key(KeyEvent {
                    code: KeyCode::Enter,
                    ..
                })
                | Event::Key(KeyEvent {
                    code: KeyCode::Char('y' | 'Y'),
                    modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                    ..
                }) => return Ok(Confirmation::Yes),

                Event::Key(KeyEvent {
                    code: KeyCode::Char('n' | 'N'),
                    modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                    ..
                }) => return Ok(Confirmation::No),

                Event::Key(KeyEvent {
                    code: KeyCode::Char('q' | 'Q'),
                    modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                    ..
                }) => return Ok(Confirmation::Quit),

                Event::Resize(w, h) => {
                    self.width = w;
                    self.height = h;
                    self.set_scroll_region()?;
                }

                _ => {} // ignore other events
            }
        }
    }

    /// Reset terminal state: restore scroll region and disable raw mode.
    pub fn cleanup(&mut self) -> io::Result<()> {
        write!(self.writer, "\x1b[r")?;
        write!(self.writer, "\x1b[{};1H\r\n", self.height)?;
        self.writer.flush()?;
        self.backend.disable_raw_mode()?;
        Ok(())
    }
}

impl<W: Write, B: TerminalBackend> Drop for PinnedPrompt<W, B> {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

/// Format a step prompt showing the current state.
pub fn format_initial_prompt(current_state: &str) -> String {
    format!("  state: {current_state}  [Y/n] step?")
}

/// Format a step prompt showing the transition from current to next state.
pub fn format_transition_prompt(current_state: &str, next_state: &str) -> String {
    format!("  state: {current_state} \u{2192} {next_state}  [Y/n]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Test backend that returns canned events and a fixed terminal size.
    struct MockBackend {
        events: VecDeque<Event>,
        width: u16,
        height: u16,
    }

    impl MockBackend {
        fn new(width: u16, height: u16, events: Vec<Event>) -> Self {
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

    fn key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent::from(code))
    }

    fn make_prompt(
        width: u16,
        height: u16,
        events: Vec<Event>,
    ) -> PinnedPrompt<Vec<u8>, MockBackend> {
        PinnedPrompt::with_backend(Vec::new(), MockBackend::new(width, height, events))
            .expect("mock prompt should initialize")
    }

    // ── Confirmation enum tests ─────────────────────────────────────────

    #[test]
    fn confirmation_variants_are_distinct() {
        assert_ne!(Confirmation::Yes, Confirmation::No);
        assert_ne!(Confirmation::Yes, Confirmation::Quit);
        assert_ne!(Confirmation::No, Confirmation::Quit);
    }

    // ── Format helper tests ─────────────────────────────────────────────

    #[test]
    fn format_initial_prompt_format_is_stable() {
        assert_eq!(
            format_initial_prompt("draft"),
            "  state: draft  [Y/n] step?"
        );
    }

    #[test]
    fn format_transition_prompt_format_is_stable() {
        assert_eq!(
            format_transition_prompt("draft", "review"),
            "  state: draft \u{2192} review  [Y/n]"
        );
    }

    // ── Initialization tests ────────────────────────────────────────────

    #[test]
    fn new_prompt_writes_scroll_region_and_clears_prompt_line() {
        let prompt = make_prompt(80, 24, vec![]);
        let output = String::from_utf8_lossy(&prompt.writer);
        // Should set scroll region to rows 1..23
        assert!(output.contains("\x1b[1;23r"), "missing DECSTBM");
        // Should position cursor in scroll region
        assert!(output.contains("\x1b[23;1H"), "missing cursor position");
        // Should clear the prompt line (row 24)
        assert!(output.contains("\x1b[24;1H\x1b[2K"), "missing clear");
    }

    #[test]
    fn tiny_terminal_skips_scroll_region() {
        // Height 1 means we can't split into scroll region + prompt line
        let prompt = make_prompt(80, 1, vec![]);
        let output = String::from_utf8_lossy(&prompt.writer);
        // DECSTBM sets a scroll region like \x1b[1;Nr — should not appear
        assert!(
            !output.contains("\x1b[1;0r"),
            "should skip DECSTBM for height 1"
        );
    }

    // ── Log output tests ────────────────────────────────────────────────

    #[test]
    fn log_writes_line_in_scroll_region() {
        let mut prompt = make_prompt(80, 24, vec![]);
        prompt.writer.clear();
        prompt.log("hello world").unwrap();
        let output = String::from_utf8_lossy(&prompt.writer);
        assert!(output.contains("hello world"));
        // Should position at bottom of scroll region before writing
        assert!(output.contains("\x1b[23;1H"));
    }

    // ── Show prompt tests ───────────────────────────────────────────────

    #[test]
    fn show_prompt_writes_on_last_line() {
        let mut prompt = make_prompt(80, 24, vec![]);
        prompt.writer.clear();
        prompt.show_prompt("step? [Y/n]").unwrap();
        let output = String::from_utf8_lossy(&prompt.writer);
        assert!(output.contains("step? [Y/n]"));
        // Should target the prompt line (row 24)
        assert!(output.contains("\x1b[24;1H\x1b[2K"));
    }

    #[test]
    fn show_prompt_truncates_to_terminal_width() {
        let mut prompt = make_prompt(10, 24, vec![]);
        prompt.writer.clear();
        prompt
            .show_prompt("this is a very long prompt that exceeds width")
            .unwrap();
        let output = String::from_utf8_lossy(&prompt.writer);
        // Should be truncated to 10 chars
        assert!(output.contains("this is a "));
        assert!(!output.contains("very long"));
    }

    // ── read_confirmation tests ─────────────────────────────────────────

    #[test]
    fn enter_returns_yes() {
        let mut prompt = make_prompt(80, 24, vec![key_event(KeyCode::Enter)]);
        assert_eq!(prompt.read_confirmation().unwrap(), Confirmation::Yes);
    }

    #[test]
    fn y_returns_yes() {
        let mut prompt = make_prompt(80, 24, vec![key_event(KeyCode::Char('y'))]);
        assert_eq!(prompt.read_confirmation().unwrap(), Confirmation::Yes);
    }

    #[test]
    fn uppercase_y_returns_yes() {
        let mut prompt = make_prompt(80, 24, vec![key_event(KeyCode::Char('Y'))]);
        assert_eq!(prompt.read_confirmation().unwrap(), Confirmation::Yes);
    }

    #[test]
    fn n_returns_no() {
        let mut prompt = make_prompt(80, 24, vec![key_event(KeyCode::Char('n'))]);
        assert_eq!(prompt.read_confirmation().unwrap(), Confirmation::No);
    }

    #[test]
    fn q_returns_quit() {
        let mut prompt = make_prompt(80, 24, vec![key_event(KeyCode::Char('q'))]);
        assert_eq!(prompt.read_confirmation().unwrap(), Confirmation::Quit);
    }

    #[test]
    fn ctrl_c_returns_quit() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let mut prompt = make_prompt(80, 24, vec![event]);
        assert_eq!(prompt.read_confirmation().unwrap(), Confirmation::Quit);
    }

    #[test]
    fn unrecognised_keys_are_skipped() {
        let events = vec![
            key_event(KeyCode::Char('x')), // ignored
            key_event(KeyCode::Char('z')), // ignored
            key_event(KeyCode::Char('y')), // accepted
        ];
        let mut prompt = make_prompt(80, 24, events);
        assert_eq!(prompt.read_confirmation().unwrap(), Confirmation::Yes);
    }

    #[test]
    fn resize_event_updates_dimensions() {
        let events = vec![Event::Resize(120, 40), key_event(KeyCode::Char('n'))];
        let mut prompt = make_prompt(80, 24, events);
        prompt.writer.clear();
        assert_eq!(prompt.read_confirmation().unwrap(), Confirmation::No);
        // After resize, dimensions should be updated
        assert_eq!(prompt.width, 120);
        assert_eq!(prompt.height, 40);
        // Should have re-set the scroll region for the new size
        let output = String::from_utf8_lossy(&prompt.writer);
        assert!(
            output.contains("\x1b[1;39r"),
            "should set new scroll region"
        );
    }

    // ── Cleanup tests ───────────────────────────────────────────────────

    #[test]
    fn cleanup_resets_scroll_region() {
        let mut prompt = make_prompt(80, 24, vec![]);
        prompt.writer.clear();
        prompt.cleanup().unwrap();
        let output = String::from_utf8_lossy(&prompt.writer);
        assert!(output.contains("\x1b[r"), "should reset scroll region");
    }
}
