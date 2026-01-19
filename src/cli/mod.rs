use anyhow::{Context, Result};
use std::process::Command;

/// wezterm CLI ラッパー
pub struct WeztermCli;

impl WeztermCli {
    /// 指定した pane にフォーカスを移動
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

    /// 指定した tab にフォーカスを移動
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // wezterm CLI が必要なので CI では skip
    fn test_activate_pane() {
        // 現在のペインを取得
        use crate::datasource::{PaneDataSource, WeztermDataSource};

        let ds = WeztermDataSource::new();
        let panes = ds.list_panes().unwrap();

        // アクティブなペインを探す
        let active_pane = panes.iter().find(|p| p.is_active);

        if let Some(pane) = active_pane {
            // 同じペインをもう一度 activate (成功するはず)
            let result = WeztermCli::activate_pane(pane.pane_id);
            assert!(result.is_ok());
        }
    }

    #[test]
    #[ignore]
    fn test_activate_nonexistent_pane() {
        // 存在しない pane_id を指定
        let result = WeztermCli::activate_pane(99999);
        assert!(result.is_err());
    }
}
