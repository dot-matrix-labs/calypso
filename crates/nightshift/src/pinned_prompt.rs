//! Pinned-prompt UI for step-through execution mode.
//!
//! Renders a fixed prompt on the last terminal line while log output scrolls
//! above it using ANSI scroll regions (DECSTBM).

use std::io::{self, Stdout, Write};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;

/// Result of reading a confirmation keypress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Yes,
    No,
    Quit,
}

/// A pinned prompt that stays on the last terminal line while log output
/// scrolls above it.
pub struct PinnedPrompt {
    stdout: Stdout,
    height: u16,
    width: u16,
}

impl PinnedPrompt {
    /// Create a new pinned prompt and set up the terminal scroll region.
    pub fn new() -> io::Result<Self> {
        let (width, height) = terminal::size()?;
        terminal::enable_raw_mode()?;

        let mut prompt = PinnedPrompt {
            stdout: io::stdout(),
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
        write!(self.stdout, "\x1b[1;{}r", self.height - 1)?;
        // Move cursor into the scroll region
        write!(self.stdout, "\x1b[{};1H", self.height - 1)?;
        self.stdout.flush()
    }

    /// Clear the prompt (bottom) line.
    fn clear_prompt_line(&mut self) -> io::Result<()> {
        write!(self.stdout, "\x1b[{};1H\x1b[2K", self.height)?;
        self.stdout.flush()
    }

    /// Write a log line above the prompt (within the scroll region).
    pub fn log(&mut self, line: &str) -> io::Result<()> {
        // Position cursor at the bottom of the scroll region so new text
        // causes a scroll-up.
        write!(self.stdout, "\x1b[{};1H", self.height - 1)?;
        // Newline triggers the scroll, then write the log line.
        write!(self.stdout, "\r\n{}", line)?;
        self.stdout.flush()
    }

    /// Update and display the prompt text on the pinned bottom line.
    pub fn show_prompt(&mut self, text: &str) -> io::Result<()> {
        // Truncate to terminal width
        let display: String = text.chars().take(self.width as usize).collect();
        write!(self.stdout, "\x1b[{};1H\x1b[2K{}", self.height, display)?;
        self.stdout.flush()
    }

    /// Read a single confirmation keypress (Y/Enter = Yes, n = No, q = Quit).
    pub fn read_confirmation(&mut self) -> io::Result<Confirmation> {
        loop {
            // Check for terminal resize
            if let Ok((w, h)) = terminal::size()
                && (w != self.width || h != self.height)
            {
                self.width = w;
                self.height = h;
                self.set_scroll_region()?;
            }

            let event = crossterm::event::read()?;
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
        // Reset scroll region
        write!(self.stdout, "\x1b[r")?;
        // Move cursor below the former prompt line
        write!(self.stdout, "\x1b[{};1H\r\n", self.height)?;
        self.stdout.flush()?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}

impl Drop for PinnedPrompt {
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

    #[test]
    fn confirmation_variants_are_distinct() {
        assert_ne!(Confirmation::Yes, Confirmation::No);
        assert_ne!(Confirmation::Yes, Confirmation::Quit);
        assert_ne!(Confirmation::No, Confirmation::Quit);
    }

    #[test]
    fn format_initial_prompt_contains_state() {
        let prompt = format_initial_prompt("new");
        assert!(prompt.contains("new"));
        assert!(prompt.contains("[Y/n]"));
        assert!(prompt.contains("step?"));
    }

    #[test]
    fn format_transition_prompt_contains_both_states() {
        let prompt = format_transition_prompt("new", "planning");
        assert!(prompt.contains("new"));
        assert!(prompt.contains("planning"));
        assert!(prompt.contains("[Y/n]"));
        assert!(prompt.contains("\u{2192}"));
    }

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
}
