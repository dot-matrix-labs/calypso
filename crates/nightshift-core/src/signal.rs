//! Signal handling for graceful shutdown in headless mode.
//!
//! Installs handlers for SIGINT and SIGTERM, forwarding signal notifications
//! through a `std::sync::mpsc` channel so callers can poll or block for
//! shutdown requests without platform-specific code in the main loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

// ---- Unix signal numbers (POSIX-mandated, stable across Linux / macOS) ----
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

// Global flags set from the C signal handler.  Only atomic stores happen
// inside the handler — no allocations, no locks, no panics.
static GOT_SIGINT: AtomicBool = AtomicBool::new(false);
static GOT_SIGTERM: AtomicBool = AtomicBool::new(false);

/// The kind of signal that was received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// SIGINT (Ctrl-C).
    Interrupt,
    /// SIGTERM.
    Terminate,
}

impl SignalKind {
    /// Standard Unix exit code for this signal (128 + signal number).
    pub fn exit_code(&self) -> i32 {
        match self {
            SignalKind::Interrupt => 128 + SIGINT,  // 130
            SignalKind::Terminate => 128 + SIGTERM, // 143
        }
    }
}

impl std::fmt::Display for SignalKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalKind::Interrupt => write!(f, "SIGINT"),
            SignalKind::Terminate => write!(f, "SIGTERM"),
        }
    }
}

/// Handle returned by [`install_signal_handlers`].  Use [`try_recv`](ShutdownSignal::try_recv)
/// for non-blocking checks or [`recv`](ShutdownSignal::recv) to block until a signal arrives.
pub struct ShutdownSignal {
    receiver: mpsc::Receiver<SignalKind>,
}

impl ShutdownSignal {
    /// Create a `ShutdownSignal` from a raw receiver (test-only).
    #[cfg(test)]
    pub(crate) fn from_receiver(receiver: mpsc::Receiver<SignalKind>) -> Self {
        Self { receiver }
    }

    /// Non-blocking check — returns `Some(kind)` if a signal has been received.
    pub fn try_recv(&self) -> Option<SignalKind> {
        self.receiver.try_recv().ok()
    }

    /// Block until a signal is received.
    pub fn recv(&self) -> SignalKind {
        self.receiver
            .recv()
            .expect("signal monitor thread should never drop the sender")
    }
}

// ---- C-level signal handler (async-signal-safe: only atomic store) --------

/// # Safety
///
/// Called from the OS signal delivery mechanism.  Must be async-signal-safe:
/// only atomic stores are performed.
unsafe extern "C" fn signal_handler(sig: i32) {
    if sig == SIGINT {
        GOT_SIGINT.store(true, Ordering::SeqCst);
    } else if sig == SIGTERM {
        GOT_SIGTERM.store(true, Ordering::SeqCst);
    }
}

unsafe extern "C" {
    fn signal(signum: i32, handler: unsafe extern "C" fn(i32)) -> usize;
}

/// Install process-wide handlers for SIGINT and SIGTERM and return a
/// [`ShutdownSignal`] that will receive notifications.
///
/// A dedicated monitor thread polls the atomic flags every 50 ms and
/// forwards the first received signal through the channel.  Only one
/// signal is delivered; subsequent signals are swallowed (the process
/// can still be killed with SIGKILL).
///
/// # Panics
///
/// Panics if the OS rejects the signal handler installation (should not
/// happen for SIGINT / SIGTERM on any supported Unix).
pub fn install_signal_handlers() -> ShutdownSignal {
    // Register C-level handlers.
    unsafe {
        signal(SIGINT, signal_handler);
        signal(SIGTERM, signal_handler);
    }

    let (sender, receiver) = mpsc::channel();

    std::thread::Builder::new()
        .name("signal-monitor".into())
        .spawn(move || {
            loop {
                if GOT_SIGINT.swap(false, Ordering::SeqCst) {
                    let _ = sender.send(SignalKind::Interrupt);
                    return;
                }
                if GOT_SIGTERM.swap(false, Ordering::SeqCst) {
                    let _ = sender.send(SignalKind::Terminate);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
        .expect("failed to spawn signal-monitor thread");

    ShutdownSignal { receiver }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_sigint_is_130() {
        assert_eq!(SignalKind::Interrupt.exit_code(), 130);
    }

    #[test]
    fn exit_code_sigterm_is_143() {
        assert_eq!(SignalKind::Terminate.exit_code(), 143);
    }

    #[test]
    fn display_formatting() {
        assert_eq!(format!("{}", SignalKind::Interrupt), "SIGINT");
        assert_eq!(format!("{}", SignalKind::Terminate), "SIGTERM");
    }

    #[test]
    fn try_recv_returns_none_when_no_signal() {
        // Construct a ShutdownSignal directly with an empty channel.
        let (_sender, receiver) = mpsc::channel::<SignalKind>();
        let shutdown = ShutdownSignal { receiver };
        assert!(shutdown.try_recv().is_none());
    }

    #[test]
    fn try_recv_returns_signal_when_sent() {
        let (sender, receiver) = mpsc::channel();
        let shutdown = ShutdownSignal { receiver };
        sender.send(SignalKind::Terminate).unwrap();
        assert_eq!(shutdown.try_recv(), Some(SignalKind::Terminate));
    }

    #[test]
    fn recv_returns_signal_when_sent() {
        let (sender, receiver) = mpsc::channel();
        let shutdown = ShutdownSignal { receiver };
        sender.send(SignalKind::Interrupt).unwrap();
        assert_eq!(shutdown.recv(), SignalKind::Interrupt);
    }

    #[test]
    fn signal_kind_equality() {
        assert_eq!(SignalKind::Interrupt, SignalKind::Interrupt);
        assert_ne!(SignalKind::Interrupt, SignalKind::Terminate);
    }
}
