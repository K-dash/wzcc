use crate::datasource::PaneDataSource;
use crate::models::Pane;
use anyhow::{Context, Result};
use std::process::Command;

/// wezterm CLI からペイン情報を取得するデータソース
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

    /// 現在の workspace 名を取得
    pub fn get_current_workspace(&self) -> Result<String> {
        // 環境変数から現在の pane_id を取得
        let current_pane_id = std::env::var("WEZTERM_PANE")
            .context("WEZTERM_PANE environment variable not set")?
            .parse::<u32>()
            .context("Failed to parse WEZTERM_PANE as u32")?;

        // すべての panes を取得
        let panes = self.list_panes()?;

        // 現在の pane を探して workspace を返す
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
    #[ignore] // wezterm CLI が必要なので CI では skip
    fn test_list_panes() {
        let ds = WeztermDataSource::new();
        let panes = ds.list_panes().unwrap();

        // 少なくとも1つのペインがあるはず (このテストを実行しているペイン自身)
        assert!(!panes.is_empty());

        // 最初のペインの構造を確認
        let first = &panes[0];
        // pane_id is u64, so just check it exists (always >= 0)
        let _ = first.pane_id;
        assert!(!first.title.is_empty());
    }
}
