use crate::datasource::{ProcessDataSource, ProcessInfo, ProcessTree};
use crate::detector::DetectionReason;
use crate::models::Pane;
use anyhow::Result;

/// Claude Code detector
pub struct ClaudeCodeDetector {
    /// Process names to detect (allowlist)
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

    /// Customize allowlist
    pub fn with_process_names(mut self, names: Vec<String>) -> Self {
        self.process_names = names;
        self
    }

    /// Case 2: Detect Claude Code by TTY matching
    ///
    /// Match pane's tty_name with ps TTY and check if process name is in allowlist
    /// Also detects wrapper-launched processes using process tree
    pub fn detect_by_tty<P: ProcessDataSource>(
        &self,
        pane: &Pane,
        process_ds: &P,
    ) -> Result<Option<DetectionReason>> {
        let tree = process_ds.build_tree()?;
        self.detect_by_tty_with_tree(pane, &tree)
    }

    /// Detect using pre-built process tree (performance optimized)
    pub fn detect_by_tty_with_tree(
        &self,
        pane: &Pane,
        tree: &ProcessTree,
    ) -> Result<Option<DetectionReason>> {
        // Exclude own pane (the pane running wzcc)
        if let Ok(current_pane_id) = std::env::var("WEZTERM_PANE") {
            if let Ok(current_id) = current_pane_id.parse::<u32>() {
                if pane.pane_id == current_id {
                    return Ok(None);
                }
            }
        }

        // Cannot use Case 2 if pane has no tty_name
        let pane_tty_short = match pane.tty_short() {
            Some(tty) => tty,
            None => return Ok(None),
        };

        // Search for processes with matching TTY
        for (pid, proc) in tree.processes.iter() {
            // Skip if process has no TTY
            let proc_tty = match &proc.tty {
                Some(tty) => tty,
                None => continue,
            };

            // Skip if TTY doesn't match
            if proc_tty != &pane_tty_short {
                continue;
            }

            // Check if process name is in allowlist (direct)
            if self.is_claude_process(proc) {
                return Ok(Some(DetectionReason::DirectTtyMatch {
                    process_name: proc.command.clone(),
                }));
            }

            // Check if parent process has claude using process tree (wrapper support)
            for name in &self.process_names {
                if tree.has_ancestor(*pid, name) {
                    return Ok(Some(DetectionReason::WrapperDetected {
                        wrapper_process: proc.command.clone(),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Check if process is Claude Code (allowlist check)
    fn is_claude_process(&self, proc: &ProcessInfo) -> bool {
        let command_lower = proc.command.to_lowercase();

        for name in &self.process_names {
            if command_lower.contains(&name.to_lowercase()) {
                return true;
            }
        }

        // Also check in args
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

    fn create_pane(pane_id: u32, tty_name: Option<&str>) -> Pane {
        Pane {
            pane_id,
            tab_id: 0,
            window_id: 0,
            workspace: "default".to_string(),
            title: "test".to_string(),
            cwd: Some("file:///Users/test/project".to_string()),
            tty_name: tty_name.map(|s| s.to_string()),
            is_active: false,
            tab_title: None,
            window_title: None,
        }
    }

    fn create_process(pid: u32, ppid: u32, tty: Option<&str>, command: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid,
            tty: tty.map(|s| s.to_string()),
            command: command.to_string(),
            args: None,
        }
    }

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
    fn test_is_claude_process_with_args() {
        let detector = ClaudeCodeDetector::new();

        // claude in args but not in command
        let proc = ProcessInfo {
            pid: 123,
            ppid: 1,
            tty: Some("ttys001".to_string()),
            command: "node".to_string(),
            args: Some("/path/to/claude code".to_string()),
        };

        assert!(detector.is_claude_process(&proc));
    }

    #[test]
    fn test_is_claude_process_case_insensitive() {
        let detector = ClaudeCodeDetector::new();

        let proc = ProcessInfo {
            pid: 123,
            ppid: 1,
            tty: Some("ttys001".to_string()),
            command: "CLAUDE".to_string(),
            args: None,
        };

        assert!(detector.is_claude_process(&proc));
    }

    #[test]
    fn test_detect_direct_tty_match() {
        let detector = ClaudeCodeDetector::new();
        let pane = create_pane(1, Some("/dev/ttys001"));

        let processes = vec![
            create_process(100, 1, Some("ttys001"), "claude"),
        ];
        let tree = ProcessTree::build(processes);

        let result = detector.detect_by_tty_with_tree(&pane, &tree).unwrap();
        assert!(matches!(result, Some(DetectionReason::DirectTtyMatch { .. })));
    }

    #[test]
    fn test_detect_no_match_different_tty() {
        let detector = ClaudeCodeDetector::new();
        let pane = create_pane(1, Some("/dev/ttys001"));

        let processes = vec![
            create_process(100, 1, Some("ttys002"), "claude"),
        ];
        let tree = ProcessTree::build(processes);

        let result = detector.detect_by_tty_with_tree(&pane, &tree).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_no_match_not_claude() {
        let detector = ClaudeCodeDetector::new();
        let pane = create_pane(1, Some("/dev/ttys001"));

        let processes = vec![
            create_process(100, 1, Some("ttys001"), "bash"),
        ];
        let tree = ProcessTree::build(processes);

        let result = detector.detect_by_tty_with_tree(&pane, &tree).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_wrapper_detected() {
        let detector = ClaudeCodeDetector::new();
        let pane = create_pane(1, Some("/dev/ttys001"));

        // bash -> shell -> claude (ancestor)
        // TTY matches on shell process, but claude is ancestor
        let processes = vec![
            create_process(100, 1, None, "claude"),      // ancestor
            create_process(200, 100, Some("ttys001"), "shell"),  // child with matching TTY
        ];
        let tree = ProcessTree::build(processes);

        let result = detector.detect_by_tty_with_tree(&pane, &tree).unwrap();
        assert!(matches!(result, Some(DetectionReason::WrapperDetected { .. })));
    }

    #[test]
    fn test_detect_pane_without_tty() {
        let detector = ClaudeCodeDetector::new();
        let pane = create_pane(1, None);  // No TTY

        let processes = vec![
            create_process(100, 1, Some("ttys001"), "claude"),
        ];
        let tree = ProcessTree::build(processes);

        let result = detector.detect_by_tty_with_tree(&pane, &tree).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_process_without_tty() {
        let detector = ClaudeCodeDetector::new();
        let pane = create_pane(1, Some("/dev/ttys001"));

        let processes = vec![
            create_process(100, 1, None, "claude"),  // No TTY
        ];
        let tree = ProcessTree::build(processes);

        let result = detector.detect_by_tty_with_tree(&pane, &tree).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_custom_allowlist() {
        let detector = ClaudeCodeDetector::new()
            .with_process_names(vec!["myapp".to_string()]);
        let pane = create_pane(1, Some("/dev/ttys001"));

        let processes = vec![
            create_process(100, 1, Some("ttys001"), "myapp"),
        ];
        let tree = ProcessTree::build(processes);

        let result = detector.detect_by_tty_with_tree(&pane, &tree).unwrap();
        assert!(matches!(result, Some(DetectionReason::DirectTtyMatch { .. })));
    }

    #[test]
    #[ignore]
    fn test_detect_by_tty() {
        // Test for when there's an actual running Claude Code session
        use crate::datasource::{PaneDataSource, WeztermDataSource};

        let detector = ClaudeCodeDetector::new();
        let pane_ds = WeztermDataSource::new();
        let process_ds = SystemProcessDataSource::new();

        let panes = pane_ds.list_panes().unwrap();

        // Find pane with "✳" in title (likely Claude Code)
        let claude_pane = panes.iter().find(|p| p.title.contains("✳"));

        if let Some(pane) = claude_pane {
            let reason = detector.detect_by_tty(pane, &process_ds).unwrap();
            println!(
                "Pane {} ({}): reason = {:?}",
                pane.pane_id, pane.title, reason
            );

            // This pane should be Claude Code
            assert!(reason.is_some());
        }
    }
}
