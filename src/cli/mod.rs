mod install_bridge;
mod install_workspace_switcher;

pub use install_bridge::{install_bridge, uninstall_bridge};
pub use install_workspace_switcher::{
    install_workspace_switcher, switch_workspace, uninstall_workspace_switcher,
};

use anyhow::{Context, Result};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const WEZTERM_CLI_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn run_command_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
    action: &str,
) -> Result<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to start {} {}", program, action))?;

    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("Failed while waiting for {} {}", program, action))?
        {
            let output = child
                .wait_with_output()
                .with_context(|| format!("Failed to collect output from {} {}", program, action))?;

            if !status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let message = if stderr.is_empty() {
                    "(no stderr output)".to_string()
                } else {
                    stderr
                };
                anyhow::bail!("{} {} failed: {}", program, action, message);
            }

            return Ok(output);
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "{} {} timed out after {}ms",
                program,
                action,
                timeout.as_millis()
            );
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn run_wezterm_cli(args: &[&str], timeout: Duration, action: &str) -> Result<Output> {
    run_command_with_timeout("wezterm", args, timeout, action)
}

/// Wezterm CLI wrapper
pub struct WeztermCli;

impl WeztermCli {
    /// Move focus to the specified pane
    pub fn activate_pane(pane_id: u32) -> Result<()> {
        let pane_id_str = pane_id.to_string();
        run_wezterm_cli(
            &["cli", "activate-pane", "--pane-id", &pane_id_str],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli activate-pane --pane-id {}", pane_id),
        )?;
        Ok(())
    }

    /// Move focus to the specified tab
    pub fn activate_tab(tab_id: u32) -> Result<()> {
        let tab_id_str = tab_id.to_string();
        run_wezterm_cli(
            &["cli", "activate-tab", "--tab-id", &tab_id_str],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli activate-tab --tab-id {}", tab_id),
        )?;
        Ok(())
    }

    /// Send text to the specified pane via bracketed paste, then press Enter to submit
    pub fn send_text(pane_id: u32, text: &str) -> Result<()> {
        let pane_id_str = pane_id.to_string();
        run_wezterm_cli(
            &["cli", "send-text", "--pane-id", &pane_id_str, "--", text],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli send-text --pane-id {} -- <text>", pane_id),
        )?;

        // Wait for the pane to process the bracketed paste before sending Enter
        std::thread::sleep(std::time::Duration::from_millis(100));

        run_wezterm_cli(
            &[
                "cli",
                "send-text",
                "--pane-id",
                &pane_id_str,
                "--no-paste",
                "\r",
            ],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli send-text --pane-id {} --no-paste <CR>", pane_id),
        )?;

        Ok(())
    }

    /// Kill (close) the specified pane
    pub fn kill_pane(pane_id: u32) -> Result<()> {
        let pane_id_str = pane_id.to_string();
        run_wezterm_cli(
            &["cli", "kill-pane", "--pane-id", &pane_id_str],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli kill-pane --pane-id {}", pane_id),
        )?;
        Ok(())
    }

    /// Split the specified pane and run a program in the new pane.
    /// Returns the pane_id of the newly created pane.
    ///
    /// The command is executed via the user's shell (`$SHELL -ic "..."`) so that
    /// shell aliases and functions are available. Each argument is shell-quoted
    /// to prevent injection and preserve arguments containing spaces.
    ///
    /// `direction` should be `"--right"` or `"--bottom"`.
    /// Expected stdout format from `wezterm cli split-pane`: a single integer (e.g., "42\n")
    pub fn split_pane(
        pane_id: u32,
        cwd: &str,
        prog: &str,
        args: &[String],
        direction: &str,
    ) -> Result<u32> {
        let (shell, shell_cmd) = build_shell_command(prog, args);

        let pane_id_str = pane_id.to_string();
        let output = run_wezterm_cli(
            &[
                "cli",
                "split-pane",
                "--pane-id",
                &pane_id_str,
                direction,
                "--cwd",
                cwd,
                "--",
                &shell,
                "-ic",
                &shell_cmd,
            ],
            WEZTERM_CLI_TIMEOUT,
            &format!(
                "cli split-pane --pane-id {} {} --cwd {}",
                pane_id, direction, cwd
            ),
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_pane_id(&stdout)
    }

    /// Spawn a new tab with the given working directory and program.
    /// Returns the pane_id of the newly created pane.
    ///
    /// The new tab is created in the same window as the source pane so that it
    /// appears in the correct workspace rather than in wzcc's own window.
    ///
    /// Like `split_pane`, the command is executed via `$SHELL -ic` for alias support.
    /// Expected stdout format from `wezterm cli spawn`: a single integer (e.g., "42\n")
    pub fn spawn_tab(cwd: &str, window_id: u32, prog: &str, args: &[String]) -> Result<u32> {
        let (shell, shell_cmd) = build_shell_command(prog, args);
        let window_id_str = window_id.to_string();

        let output = run_wezterm_cli(
            &[
                "cli",
                "spawn",
                "--cwd",
                cwd,
                "--window-id",
                &window_id_str,
                "--",
                &shell,
                "-ic",
                &shell_cmd,
            ],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli spawn --cwd {} --window-id {}", cwd, window_id),
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_pane_id(&stdout)
    }

    /// Retrieve the textual content of a pane including ANSI escape sequences.
    /// Returns the raw stdout bytes because `ansi-to-tui` works with `&[u8]`.
    pub fn get_text(pane_id: u32) -> Result<Vec<u8>> {
        let pane_id_str = pane_id.to_string();
        let output = run_wezterm_cli(
            &["cli", "get-text", "--pane-id", &pane_id_str, "--escapes"],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli get-text --pane-id {} --escapes", pane_id),
        )?;
        Ok(output.stdout)
    }

    /// Retrieve the textual content of a pane as plain text (no ANSI escapes).
    pub fn get_text_plain(pane_id: u32) -> Result<String> {
        let pane_id_str = pane_id.to_string();
        let output = run_wezterm_cli(
            &["cli", "get-text", "--pane-id", &pane_id_str],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli get-text --pane-id {}", pane_id),
        )?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Change tab title for the specified pane
    pub fn set_tab_title(pane_id: u32, title: &str) -> Result<()> {
        let pane_id_str = pane_id.to_string();
        run_wezterm_cli(
            &["cli", "set-tab-title", "--pane-id", &pane_id_str, title],
            WEZTERM_CLI_TIMEOUT,
            &format!("cli set-tab-title --pane-id {}", pane_id),
        )?;
        Ok(())
    }
}

/// Build a shell command string for spawning via `$SHELL -ic "..."`.
/// Returns (shell_path, shell_command_string).
/// The first element (prog) is left unquoted to allow alias/function resolution.
/// Subsequent args are shell-quoted to preserve spaces and prevent injection.
fn build_shell_command(prog: &str, args: &[String]) -> (String, String) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let shell_cmd = if args.is_empty() {
        prog.to_string()
    } else {
        let quoted_args: Vec<String> = args.iter().map(|a| shell_quote(a)).collect();
        format!("{} {}", prog, quoted_args.join(" "))
    };
    (shell, shell_cmd)
}

/// Shell-quote a string using POSIX single-quote escaping.
/// Wraps in single quotes and replaces internal `'` with `'\''`.
/// e.g. `hello world` → `'hello world'`, `it's` → `'it'\''s'`
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Parse pane-id from wezterm cli spawn stdout output.
/// Expected format: a single integer, optionally followed by whitespace/newline.
fn parse_pane_id(stdout: &str) -> Result<u32> {
    stdout
        .trim()
        .parse::<u32>()
        .context("Failed to parse pane-id from wezterm cli spawn output")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_shell_quote_simple() {
        assert_eq!(shell_quote("hello"), "'hello'");
    }

    #[test]
    fn test_shell_quote_with_spaces() {
        assert_eq!(shell_quote("a b"), "'a b'");
    }

    #[test]
    fn test_shell_quote_with_single_quote() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_shell_quote_with_special_chars() {
        assert_eq!(shell_quote("; rm -rf /"), "'; rm -rf /'");
        assert_eq!(shell_quote("$(whoami)"), "'$(whoami)'");
    }

    #[test]
    fn test_parse_pane_id_valid() {
        assert_eq!(parse_pane_id("42\n").unwrap(), 42);
        assert_eq!(parse_pane_id("0").unwrap(), 0);
        assert_eq!(parse_pane_id("  123  \n").unwrap(), 123);
    }

    #[test]
    fn test_parse_pane_id_invalid() {
        assert!(parse_pane_id("").is_err());
        assert!(parse_pane_id("abc").is_err());
        assert!(parse_pane_id("-1").is_err());
        assert!(parse_pane_id("42 extra").is_err());
    }

    #[test]
    fn test_run_command_with_timeout_success() {
        let output = run_command_with_timeout(
            "/bin/sh",
            &["-c", "printf 'ok'"],
            Duration::from_secs(1),
            "test-success",
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok");
    }

    #[test]
    fn test_run_command_with_timeout_expires() {
        let err = run_command_with_timeout(
            "/bin/sh",
            &["-c", "sleep 1"],
            Duration::from_millis(50),
            "test-timeout",
        )
        .unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    #[ignore] // Skip in CI (requires wezterm CLI)
    fn test_activate_pane() {
        // Get current pane
        use crate::datasource::{PaneDataSource, WeztermDataSource};

        let ds = WeztermDataSource::new();
        let panes = ds.list_panes().unwrap();

        // Find active pane
        let active_pane = panes.iter().find(|p| p.is_active);

        if let Some(pane) = active_pane {
            // Activate the same pane again (should succeed)
            let result = WeztermCli::activate_pane(pane.pane_id);
            assert!(result.is_ok());
        }
    }

    #[test]
    #[ignore]
    fn test_activate_nonexistent_pane() {
        // Specify non-existent pane_id
        let result = WeztermCli::activate_pane(99999);
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // Skip in CI (requires wezterm CLI)
    fn test_get_text() {
        use crate::datasource::{PaneDataSource, WeztermDataSource};

        let ds = WeztermDataSource::new();
        let panes = ds.list_panes().unwrap();

        if let Some(pane) = panes.iter().find(|p| p.is_active) {
            let result = WeztermCli::get_text(pane.pane_id);
            assert!(result.is_ok());
            assert!(!result.unwrap().is_empty());
        }
    }

    #[test]
    #[ignore] // Skip in CI (requires wezterm CLI)
    fn test_get_text_plain() {
        use crate::datasource::{PaneDataSource, WeztermDataSource};

        let ds = WeztermDataSource::new();
        let panes = ds.list_panes().unwrap();

        if let Some(pane) = panes.iter().find(|p| p.is_active) {
            let result = WeztermCli::get_text_plain(pane.pane_id);
            assert!(result.is_ok());
            assert!(!result.unwrap().is_empty());
        }
    }

    #[test]
    #[ignore]
    fn test_get_text_nonexistent_pane() {
        let result = WeztermCli::get_text(99999);
        assert!(result.is_err());
    }
}
