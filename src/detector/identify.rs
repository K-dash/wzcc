use crate::datasource::{ProcessDataSource, ProcessInfo};
use crate::models::Pane;
use anyhow::Result;

/// Claude Code 検出器
pub struct ClaudeCodeDetector {
    /// 検出対象のプロセス名リスト (allowlist)
    process_names: Vec<String>,
}

impl Default for ClaudeCodeDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeCodeDetector {
    pub fn new() -> Self {
        Self {
            process_names: vec!["claude".to_string(), "anthropic".to_string()],
        }
    }

    /// allowlist をカスタマイズ
    pub fn with_process_names(mut self, names: Vec<String>) -> Self {
        self.process_names = names;
        self
    }

    /// Case 2: TTY マッチングで Claude Code を検出
    ///
    /// pane の tty_name と ps の TTY を突合し、プロセス名が allowlist に含まれるかチェック
    /// Phase 2.2: プロセスツリーを使って wrapper 経由起動も検出
    pub fn detect_by_tty<P: ProcessDataSource>(&self, pane: &Pane, process_ds: &P) -> Result<bool> {
        // 自分自身のペイン (wzcc を実行しているペイン) を除外
        if let Ok(current_pane_id) = std::env::var("WEZTERM_PANE") {
            if let Ok(current_id) = current_pane_id.parse::<u32>() {
                if pane.pane_id == current_id {
                    return Ok(false);
                }
            }
        }

        // pane に tty_name がない場合は Case 2 を使えない
        let pane_tty_short = match pane.tty_short() {
            Some(tty) => tty,
            None => return Ok(false),
        };

        // 全プロセスを取得してツリーを構築
        let tree = process_ds.build_tree()?;

        // TTY が一致するプロセスを検索
        for (pid, proc) in tree.processes.iter() {
            // プロセスに TTY がない場合はスキップ
            let proc_tty = match &proc.tty {
                Some(tty) => tty,
                None => continue,
            };

            // TTY が一致しない場合はスキップ
            if proc_tty != &pane_tty_short {
                continue;
            }

            // プロセス名が allowlist に含まれるかチェック (直接)
            if self.is_claude_process(proc) {
                return Ok(true);
            }

            // Phase 2.2: プロセスツリーで親プロセスに claude があるかチェック (wrapper 対応)
            for name in &self.process_names {
                if tree.has_ancestor(*pid, name) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// プロセスが Claude Code かどうかを判定 (allowlist チェック)
    fn is_claude_process(&self, proc: &ProcessInfo) -> bool {
        let command_lower = proc.command.to_lowercase();

        for name in &self.process_names {
            if command_lower.contains(&name.to_lowercase()) {
                return true;
            }
        }

        // args にも含まれるかチェック
        if let Some(args) = &proc.args {
            let args_lower = args.to_lowercase();
            for name in &self.process_names {
                if args_lower.contains(&name.to_lowercase()) {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasource::SystemProcessDataSource;

    #[test]
    fn test_is_claude_process() {
        let detector = ClaudeCodeDetector::new();

        let claude_proc = ProcessInfo {
            pid: 123,
            ppid: 1,
            tty: Some("ttys001".to_string()),
            command: "claude".to_string(),
            args: None,
        };

        assert!(detector.is_claude_process(&claude_proc));

        let bash_proc = ProcessInfo {
            pid: 456,
            ppid: 1,
            tty: Some("ttys002".to_string()),
            command: "bash".to_string(),
            args: None,
        };

        assert!(!detector.is_claude_process(&bash_proc));
    }

    #[test]
    #[ignore]
    fn test_detect_by_tty() {
        // 実際に動いている Claude Code セッションがある場合のテスト
        use crate::datasource::{PaneDataSource, WeztermDataSource};

        let detector = ClaudeCodeDetector::new();
        let pane_ds = WeztermDataSource::new();
        let process_ds = SystemProcessDataSource::new();

        let panes = pane_ds.list_panes().unwrap();

        // title に "✳" が含まれるペインを探す (おそらく Claude Code)
        let claude_pane = panes.iter().find(|p| p.title.contains("✳"));

        if let Some(pane) = claude_pane {
            let is_claude = detector.detect_by_tty(pane, &process_ds).unwrap();
            println!(
                "Pane {} ({}): detected = {}",
                pane.pane_id, pane.title, is_claude
            );

            // この pane は Claude Code であるはず
            assert!(is_claude);
        }
    }
}
