//! PTY-based end-to-end tests for TUI surfaces.
//!
//! These tests spawn the `calypso-cli` binary in a real pseudo-terminal using
//! `expectrl`, send keystrokes, and assert on rendered screen output.
//!
//! PTY tests require a real TTY (not available under all CI runners or
//! coverage tools).  They are gated behind `#[cfg(unix)]` and marked
//! `#[ignore]` so they do not run by default.  To run them locally:
//!
//! ```sh
//! cargo test --test e2e_tui -- --ignored --nocapture
//! ```

#[cfg(unix)]
mod pty_tests {
    use std::process::Command;
    use std::time::Duration;

    use expectrl::{ControlCode, Eof, Regex, Session};

    /// The path to the compiled `calypso-cli` binary (resolved at test compile time).
    const BINARY: &str = env!("CARGO_BIN_EXE_calypso-cli");

    /// Spawn a `calypso-cli` session in a PTY with the given arguments and a
    /// temporary working directory that has no `.calypso/state.json`, so the
    /// binary falls through to the Doctor TUI.
    fn spawn_doctor_session(extra_args: &[&str]) -> Session {
        let work_dir =
            std::env::temp_dir().join(format!("calypso-pty-{}", std::process::id()));
        std::fs::create_dir_all(&work_dir).expect("temp dir should be created");

        let mut cmd = Command::new(BINARY);
        cmd.current_dir(&work_dir);
        for arg in extra_args {
            cmd.arg(arg);
        }

        let mut session = Session::spawn(cmd).expect("should spawn calypso-cli in PTY");
        session.set_expect_timeout(Some(Duration::from_secs(10)));
        session
    }

    // ── Doctor TUI tests ─────────────────────────────────────────────────

    #[test]
    #[ignore]
    fn doctor_tui_launches_and_shows_check_results() {
        let mut session = spawn_doctor_session(&[]);
        // The doctor surface renders "passing" in its header line (e.g. "3/5 passing").
        session
            .expect(Regex("passing"))
            .expect("should see 'passing' in doctor output");
        session.send("q").expect("should send q");
        session.expect(Eof).expect("process should exit after q");
    }

    #[test]
    #[ignore]
    fn doctor_tui_shows_doctor_tab_label() {
        let mut session = spawn_doctor_session(&[]);
        // The app shell renders tab labels including "Doctor" in the ribbon.
        session
            .expect(Regex("Doctor"))
            .expect("should see Doctor tab label in ribbon");
        session.send("q").expect("should send q");
        session.expect(Eof).expect("process should exit after q");
    }

    #[test]
    #[ignore]
    fn doctor_tui_exits_on_q() {
        let mut session = spawn_doctor_session(&[]);
        session
            .expect(Regex("passing"))
            .expect("should render doctor surface");
        session.send("q").expect("should send q");
        session
            .expect(Eof)
            .expect("process should exit cleanly on q");
    }

    #[test]
    #[ignore]
    fn doctor_tui_exits_on_esc() {
        let mut session = spawn_doctor_session(&[]);
        session
            .expect(Regex("passing"))
            .expect("should render doctor surface");
        // ESC is byte 0x1b
        session
            .send(&[0x1b_u8] as &[u8])
            .expect("should send ESC");
        session
            .expect(Eof)
            .expect("process should exit cleanly on ESC");
    }

    #[test]
    #[ignore]
    fn doctor_tui_exits_on_ctrl_c() {
        let mut session = spawn_doctor_session(&[]);
        session
            .expect(Regex("passing"))
            .expect("should render doctor surface");
        session
            .send(ControlCode::EndOfText)
            .expect("should send Ctrl-C");
        session
            .expect(Eof)
            .expect("process should exit cleanly on Ctrl-C");
    }

    #[test]
    #[ignore]
    fn doctor_tui_refreshes_on_r_key() {
        let mut session = spawn_doctor_session(&[]);
        session
            .expect(Regex("passing"))
            .expect("should render doctor surface initially");
        session.send("r").expect("should send r for refresh");
        // After refresh, the checks should still render.
        session
            .expect(Regex("passing"))
            .expect("should see passing after refresh");
        session.send("q").expect("should send q");
        session.expect(Eof).expect("process should exit after q");
    }

    #[test]
    #[ignore]
    fn doctor_tui_arrow_keys_navigate_without_crashing() {
        let mut session = spawn_doctor_session(&[]);
        session
            .expect(Regex("passing"))
            .expect("should render doctor surface");

        // Send Down arrow (ANSI escape: ESC [ B)
        session
            .send("\x1b[B")
            .expect("should send Down arrow");
        // Send another Down
        session
            .send("\x1b[B")
            .expect("should send Down arrow again");
        // Send Up arrow (ANSI escape: ESC [ A)
        session
            .send("\x1b[A")
            .expect("should send Up arrow");

        // Surface should still be alive and rendering after navigation.
        session
            .expect(Regex("passing"))
            .expect("should still render after arrow navigation");
        session.send("q").expect("should send q");
        session.expect(Eof).expect("process should exit after q");
    }

    // ── Non-interactive CLI paths (sanity check that PTY works for them) ─

    #[test]
    #[ignore]
    fn doctor_text_subcommand_renders_in_pty() {
        let mut session = spawn_doctor_session(&["doctor"]);
        // The non-TUI `calypso doctor` subcommand prints check results and exits.
        session
            .expect(Eof)
            .expect("doctor subcommand should complete and exit");
    }

    #[test]
    #[ignore]
    fn version_flag_renders_in_pty() {
        let work_dir =
            std::env::temp_dir().join(format!("calypso-pty-ver-{}", std::process::id()));
        std::fs::create_dir_all(&work_dir).expect("temp dir should be created");

        let mut cmd = Command::new(BINARY);
        cmd.current_dir(&work_dir).arg("--version");

        let mut session = Session::spawn(cmd).expect("should spawn calypso-cli --version");
        session.set_expect_timeout(Some(Duration::from_secs(5)));
        session
            .expect(Regex("0\\.1\\.0"))
            .expect("should print version");
        session.expect(Eof).expect("should exit after printing version");
    }
}
