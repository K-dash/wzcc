//! Install/uninstall the workspace switcher for cross-workspace navigation.
//!
//! This module provides commands to set up the WezTerm Lua configuration
//! that enables workspace switching via OSC 1337 user variables.

use anyhow::{Context, Result};
use base64::prelude::*;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Markers used to identify the injected Lua code.
const BEGIN_MARKER: &str = "-- BEGIN WZCC WORKSPACE SWITCHER --";
const END_MARKER: &str = "-- END WZCC WORKSPACE SWITCHER --";

/// The Lua snippet to inject into wezterm.lua.
/// Uses `wezterm_wzcc` as a local variable to avoid conflicts with existing `wezterm` variable.
const LUA_SNIPPET: &str = r#"-- BEGIN WZCC WORKSPACE SWITCHER --
-- Auto-added by: wzcc install-workspace-switcher
-- To remove, run: wzcc uninstall-workspace-switcher
local wezterm_wzcc = require 'wezterm'
wezterm_wzcc.on('user-var-changed', function(window, pane, name, value)
  if name == 'wzcc_switch_workspace' and value and value ~= '' then
    window:perform_action(
      wezterm_wzcc.action.SwitchToWorkspace { name = value },
      pane
    )
  end
end)
-- END WZCC WORKSPACE SWITCHER --

"#;

/// Find the WezTerm configuration file path.
///
/// Search order:
/// 1. `~/.wezterm.lua`
/// 2. `~/.config/wezterm/wezterm.lua`
/// 3. `$XDG_CONFIG_HOME/wezterm/wezterm.lua`
/// 4. If none exist, returns `~/.wezterm.lua` for creation
pub fn wezterm_config_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    // Check ~/.wezterm.lua first
    let dot_wezterm = home.join(".wezterm.lua");
    if dot_wezterm.exists() {
        return Some(dot_wezterm);
    }

    // Check ~/.config/wezterm/wezterm.lua
    let config_wezterm = home.join(".config").join("wezterm").join("wezterm.lua");
    if config_wezterm.exists() {
        return Some(config_wezterm);
    }

    // Check $XDG_CONFIG_HOME/wezterm/wezterm.lua
    if let Some(xdg_config) = dirs::config_dir() {
        let xdg_wezterm = xdg_config.join("wezterm").join("wezterm.lua");
        if xdg_wezterm.exists() {
            return Some(xdg_wezterm);
        }
    }

    // Default to ~/.wezterm.lua for creation
    Some(dot_wezterm)
}

/// Install the workspace switcher.
///
/// This function:
/// 1. Finds or creates the WezTerm config file
/// 2. Checks if the switcher is already installed
/// 3. Prepends the Lua snippet with markers
pub fn install_workspace_switcher() -> Result<()> {
    let config_path = wezterm_config_path().context("Could not determine home directory")?;

    // Read existing content or start with empty string
    let existing_content = if config_path.exists() {
        fs::read_to_string(&config_path).context("Failed to read wezterm.lua")?
    } else {
        String::new()
    };

    // Check if already installed
    if existing_content.contains(BEGIN_MARKER) {
        println!("Workspace switcher is already installed!");
        println!("  Config file: {}", config_path.display());
        return Ok(());
    }

    // Prepend the Lua snippet
    let new_content = format!("{}{}", LUA_SNIPPET, existing_content);

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).context("Failed to create config directory")?;
    }

    // Write the updated config
    fs::write(&config_path, new_content).context("Failed to write wezterm.lua")?;

    println!("Workspace switcher installed successfully!");
    println!();
    println!("  Config file: {}", config_path.display());
    println!();
    println!("Please restart WezTerm or reload config (Ctrl+Shift+R) for changes to take effect.");

    Ok(())
}

/// Uninstall the workspace switcher.
///
/// This function:
/// 1. Finds the WezTerm config file
/// 2. Removes the Lua snippet between markers
pub fn uninstall_workspace_switcher() -> Result<()> {
    let config_path = wezterm_config_path().context("Could not determine home directory")?;

    if !config_path.exists() {
        println!("WezTerm config file not found. Nothing to uninstall.");
        return Ok(());
    }

    let content = fs::read_to_string(&config_path).context("Failed to read wezterm.lua")?;

    // Check if installed
    if !content.contains(BEGIN_MARKER) {
        println!("Workspace switcher is not installed. Nothing to uninstall.");
        return Ok(());
    }

    // Remove the snippet between markers (including trailing newlines)
    let new_content = remove_between_markers(&content, BEGIN_MARKER, END_MARKER);

    // Write the updated config
    fs::write(&config_path, new_content).context("Failed to write wezterm.lua")?;

    println!("Workspace switcher uninstalled successfully!");
    println!();
    println!("  Config file: {}", config_path.display());
    println!();
    println!("Please restart WezTerm or reload config (Ctrl+Shift+R) for changes to take effect.");

    Ok(())
}

/// Remove content between markers (inclusive), plus any trailing blank lines.
fn remove_between_markers(content: &str, begin: &str, end: &str) -> String {
    let mut result = String::new();
    let mut skip = false;
    let mut skip_next_blank = true;

    for line in content.lines() {
        if line.contains(begin) {
            skip = true;
            continue;
        }
        if skip && line.contains(end) {
            skip = false;
            skip_next_blank = true;
            continue;
        }
        if skip {
            continue;
        }
        // Skip blank lines immediately after the end marker
        if skip_next_blank && line.trim().is_empty() {
            skip_next_blank = false;
            continue;
        }
        skip_next_blank = false;

        result.push_str(line);
        result.push('\n');
    }

    result
}

/// Switch to a workspace by sending an OSC 1337 escape sequence.
///
/// This requires the workspace switcher to be installed via `install_workspace_switcher`.
pub fn switch_workspace(workspace_name: &str) -> Result<()> {
    let encoded = BASE64_STANDARD.encode(workspace_name);
    print!("\x1b]1337;SetUserVar=wzcc_switch_workspace={}\x07", encoded);
    std::io::stdout()
        .flush()
        .context("Failed to flush stdout")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_snippet_contains_markers() {
        assert!(LUA_SNIPPET.contains(BEGIN_MARKER));
        assert!(LUA_SNIPPET.contains(END_MARKER));
    }

    #[test]
    fn test_lua_snippet_contains_event_handler() {
        assert!(LUA_SNIPPET.contains("user-var-changed"));
        assert!(LUA_SNIPPET.contains("wzcc_switch_workspace"));
        assert!(LUA_SNIPPET.contains("SwitchToWorkspace"));
    }

    #[test]
    fn test_remove_between_markers() {
        let content = r#"line1
-- BEGIN WZCC WORKSPACE SWITCHER --
some code
-- END WZCC WORKSPACE SWITCHER --

line2
"#;
        let result = remove_between_markers(content, BEGIN_MARKER, END_MARKER);
        assert_eq!(result, "line1\nline2\n");
    }

    #[test]
    fn test_remove_between_markers_at_start() {
        let content = r#"-- BEGIN WZCC WORKSPACE SWITCHER --
some code
-- END WZCC WORKSPACE SWITCHER --

existing config
"#;
        let result = remove_between_markers(content, BEGIN_MARKER, END_MARKER);
        assert_eq!(result, "existing config\n");
    }

    #[test]
    fn test_wezterm_config_path() {
        let path = wezterm_config_path();
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(path.ends_with(".wezterm.lua") || path.ends_with("wezterm.lua"));
    }
}
