mod install_bridge;
mod install_workspace_switcher;

pub use install_bridge::{install_bridge, uninstall_bridge};
pub use install_workspace_switcher::{
    install_workspace_switcher, switch_workspace, uninstall_workspace_switcher,
};

use anyhow::{Context, Result};
use std::process::Command;

/// Wezterm CLI wrapper
pub struct WeztermCli;

impl WeztermCli {
    /// Move focus to the specified pane
    pub fn activate_pane(pane_id: u32) -> Result<()> {
        let output = Command::new("wezterm")
            .args(["cli", "activate-pane", "--pane-id", &pane_id.to_string()])
            .output()
            .context("Failed to execute wezterm cli activate-pane")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "wezterm cli activate-pane failed for pane {}: {}",
                pane_id,
                stderr
            );
        }

        Ok(())
    }

    /// Move focus to the specified tab
    pub fn activate_tab(tab_id: u32) -> Result<()> {
        let output = Command::new("wezterm")
            .args(["cli", "activate-tab", "--tab-id", &tab_id.to_string()])
            .output()
            .context("Failed to execute wezterm cli activate-tab")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "wezterm cli activate-tab failed for tab {}: {}",
                tab_id,
                stderr
            );
        }

        Ok(())
    }

    /// Change tab title for the specified pane
    pub fn set_tab_title(pane_id: u32, title: &str) -> Result<()> {
        let output = Command::new("wezterm")
            .args([
                "cli",
                "set-tab-title",
                "--pane-id",
                &pane_id.to_string(),
                title,
            ])
            .output()
            .context("Failed to execute wezterm cli set-tab-title")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "wezterm cli set-tab-title failed for pane {}: {}",
                pane_id,
                stderr
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
