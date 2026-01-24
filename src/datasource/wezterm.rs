use crate::datasource::PaneDataSource;
use crate::models::Pane;
use anyhow::{Context, Result};
use std::process::Command;

/// Data source that retrieves pane information from wezterm CLI
pub struct WeztermDataSource;

impl Default for WeztermDataSource {
    fn default() -> Self {
        Self::new()
    }
}

impl WeztermDataSource {
    pub fn new() -> Self {
        Self
    }

    /// Get current workspace name
    pub fn get_current_workspace(&self) -> Result<String> {
        // Get current pane_id from environment variable
        let current_pane_id = std::env::var("WEZTERM_PANE")
            .context("WEZTERM_PANE environment variable not set")?
            .parse::<u32>()
            .context("Failed to parse WEZTERM_PANE as u32")?;

        // Get all panes
        let panes = self.list_panes()?;

        // Find current pane and return workspace
        panes
            .iter()
            .find(|p| p.pane_id == current_pane_id)
            .map(|p| p.workspace.clone())
            .ok_or_else(|| anyhow::anyhow!("Current pane not found in pane list"))
    }
}

impl PaneDataSource for WeztermDataSource {
    fn list_panes(&self) -> Result<Vec<Pane>> {
        let output = Command::new("wezterm")
            .args(["cli", "list", "--format", "json"])
            .output()
            .context("Failed to execute wezterm cli list")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("wezterm cli list failed: {}", stderr);
        }

        let stdout = String::from_utf8(output.stdout)
            .context("wezterm cli list output is not valid UTF-8")?;

        serde_json::from_str(&stdout).context("Failed to parse wezterm cli list output as JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Skip in CI (requires wezterm CLI)
    fn test_list_panes() {
        let ds = WeztermDataSource::new();
        let panes = ds.list_panes().unwrap();

        // Should have at least one pane (the pane running this test)
        assert!(!panes.is_empty());

        // Check first pane structure
        let first = &panes[0];
        // pane_id is u64, so just check it exists (always >= 0)
        let _ = first.pane_id;
        assert!(!first.title.is_empty());
    }
}
