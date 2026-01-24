use anyhow::{Context, Result};
use std::collections::{HashMap, VecDeque};
use std::process::Command;

/// プロセス情報
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub tty: Option<String>,
    pub command: String,
    pub args: Option<String>,
}

/// プロセスツリー
pub struct ProcessTree {
    /// 子プロセスのマップ (pid -> [child_pids])
    pub children: HashMap<u32, Vec<u32>>,
    /// 全プロセス情報 (pid -> ProcessInfo)
    pub processes: HashMap<u32, ProcessInfo>,
}

impl ProcessTree {
    /// プロセス一覧からツリーを構築
    pub fn build(processes: Vec<ProcessInfo>) -> Self {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut process_map: HashMap<u32, ProcessInfo> = HashMap::new();

        for proc in processes {
            // 親の children リストに追加
            children.entry(proc.ppid).or_default().push(proc.pid);

            // プロセス情報を保存
            process_map.insert(proc.pid, proc);
        }

        Self {
            children,
            processes: process_map,
        }
    }

    /// 指定した PID の祖先に target 文字列を含むプロセスがあるかチェック (BFS)
    pub fn has_ancestor(&self, pid: u32, target: &str) -> bool {
        let target_lower = target.to_lowercase();
        let mut visited = std::collections::HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(pid);

        while let Some(current_pid) = queue.pop_front() {
            if visited.contains(&current_pid) {
                continue;
            }
            visited.insert(current_pid);

            let Some(proc) = self.processes.get(&current_pid) else {
                continue;
            };

            // コマンド名または引数に target が含まれるかチェック
            if proc.command.to_lowercase().contains(&target_lower) {
                return true;
            }

            if let Some(args) = &proc.args {
                if args.to_lowercase().contains(&target_lower) {
                    return true;
                }
            }

            // 親プロセスをキューに追加
            if proc.ppid != 0 {
                queue.push_back(proc.ppid);
            }
        }

        false
    }

    /// 指定した PID のプロセス情報を取得
    pub fn get(&self, pid: u32) -> Option<&ProcessInfo> {
        self.processes.get(&pid)
    }
}

/// プロセスデータソースの trait
pub trait ProcessDataSource {
    fn list_processes(&self) -> Result<Vec<ProcessInfo>>;

    /// プロセスツリーを構築
    fn build_tree(&self) -> Result<ProcessTree> {
        let processes = self.list_processes()?;
        Ok(ProcessTree::build(processes))
    }
}

/// システムの ps コマンドからプロセス情報を取得
pub struct SystemProcessDataSource;

impl Default for SystemProcessDataSource {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProcessDataSource {
    pub fn new() -> Self {
        Self
    }

    /// TTY を正規化 (pts/0, ttys001 など)
    fn normalize_tty(tty: &str) -> Option<String> {
        let tty = tty.trim();

        // "?" は TTY なしを意味する
        if tty == "?" || tty.is_empty() {
            return None;
        }

        // "/dev/" プレフィックスを除去
        let tty = tty.strip_prefix("/dev/").unwrap_or(tty);

        Some(tty.to_string())
    }
}

impl ProcessDataSource for SystemProcessDataSource {
    fn list_processes(&self) -> Result<Vec<ProcessInfo>> {
        // ps -eo pid,ppid,tty,comm,args
        // macOS/Linux 共通の形式
        let output = Command::new("ps")
            .args(["-eo", "pid,ppid,tty,comm,args"])
            .output()
            .context("Failed to execute ps command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("ps command failed: {}", stderr);
        }

        // UTF-8 でない文字は ? に置換 (macOS で一部プロセスに非 UTF-8 が含まれる場合がある)
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        let mut processes = Vec::new();

        for (idx, line) in stdout.lines().enumerate() {
            // ヘッダー行をスキップ
            if idx == 0 {
                continue;
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // PID PPID TTY COMMAND ARGS の順
            let parts: Vec<&str> = line.splitn(5, ' ').filter(|s| !s.is_empty()).collect();

            if parts.len() < 4 {
                // パース失敗は無視
                continue;
            }

            let pid: u32 = match parts[0].parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let ppid: u32 = match parts[1].parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let tty = Self::normalize_tty(parts[2]);
            let command = parts[3].to_string();
            let args = parts.get(4).map(|s| s.to_string());

            processes.push(ProcessInfo {
                pid,
                ppid,
                tty,
                command,
                args,
            });
        }

        Ok(processes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_process(pid: u32, ppid: u32, command: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid,
            tty: None,
            command: command.to_string(),
            args: None,
        }
    }

    fn create_process_with_args(pid: u32, ppid: u32, command: &str, args: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid,
            tty: None,
            command: command.to_string(),
            args: Some(args.to_string()),
        }
    }

    #[test]
    fn test_normalize_tty() {
        assert_eq!(
            SystemProcessDataSource::normalize_tty("ttys003"),
            Some("ttys003".to_string())
        );
        assert_eq!(
            SystemProcessDataSource::normalize_tty("/dev/ttys003"),
            Some("ttys003".to_string())
        );
        assert_eq!(
            SystemProcessDataSource::normalize_tty("pts/0"),
            Some("pts/0".to_string())
        );
        assert_eq!(SystemProcessDataSource::normalize_tty("?"), None);
        assert_eq!(SystemProcessDataSource::normalize_tty(""), None);
    }

    #[test]
    fn test_process_tree_build() {
        let processes = vec![
            create_process(1, 0, "init"),
            create_process(100, 1, "bash"),
            create_process(200, 100, "vim"),
        ];

        let tree = ProcessTree::build(processes);

        assert_eq!(tree.processes.len(), 3);
        assert!(tree.processes.contains_key(&1));
        assert!(tree.processes.contains_key(&100));
        assert!(tree.processes.contains_key(&200));

        // Check children structure
        assert_eq!(tree.children.get(&1), Some(&vec![100]));
        assert_eq!(tree.children.get(&100), Some(&vec![200]));
    }

    #[test]
    fn test_process_tree_get() {
        let processes = vec![
            create_process(100, 1, "bash"),
        ];

        let tree = ProcessTree::build(processes);

        assert!(tree.get(100).is_some());
        assert_eq!(tree.get(100).unwrap().command, "bash");
        assert!(tree.get(999).is_none());
    }

    #[test]
    fn test_has_ancestor_direct_parent() {
        // 100 -> 200, find "bash" from 200
        let processes = vec![
            create_process(100, 1, "bash"),
            create_process(200, 100, "vim"),
        ];

        let tree = ProcessTree::build(processes);

        assert!(tree.has_ancestor(200, "bash"));
    }

    #[test]
    fn test_has_ancestor_grandparent() {
        // 100 -> 200 -> 300, find "bash" from 300
        let processes = vec![
            create_process(100, 1, "bash"),
            create_process(200, 100, "zsh"),
            create_process(300, 200, "vim"),
        ];

        let tree = ProcessTree::build(processes);

        assert!(tree.has_ancestor(300, "bash"));
    }

    #[test]
    fn test_has_ancestor_self() {
        // Check if process itself matches (it should)
        let processes = vec![
            create_process(100, 1, "claude"),
        ];

        let tree = ProcessTree::build(processes);

        assert!(tree.has_ancestor(100, "claude"));
    }

    #[test]
    fn test_has_ancestor_not_found() {
        let processes = vec![
            create_process(100, 1, "bash"),
            create_process(200, 100, "vim"),
        ];

        let tree = ProcessTree::build(processes);

        assert!(!tree.has_ancestor(200, "claude"));
    }

    #[test]
    fn test_has_ancestor_in_args() {
        // claude is in args, not command
        let processes = vec![
            create_process_with_args(100, 1, "node", "/path/to/claude"),
            create_process(200, 100, "vim"),
        ];

        let tree = ProcessTree::build(processes);

        assert!(tree.has_ancestor(200, "claude"));
    }

    #[test]
    fn test_has_ancestor_case_insensitive() {
        let processes = vec![
            create_process(100, 1, "CLAUDE"),
            create_process(200, 100, "vim"),
        ];

        let tree = ProcessTree::build(processes);

        // target is lowercased in the search
        assert!(tree.has_ancestor(200, "claude"));
    }

    #[test]
    fn test_has_ancestor_cycle_protection() {
        // Create a cycle: 100 -> 200 -> 100 (shouldn't happen but test protection)
        let processes = vec![
            create_process(100, 200, "bash"),
            create_process(200, 100, "zsh"),
        ];

        let tree = ProcessTree::build(processes);

        // Should not hang, should return false (claude not found)
        assert!(!tree.has_ancestor(100, "claude"));
    }

    #[test]
    fn test_has_ancestor_missing_parent() {
        // Parent process doesn't exist in tree
        let processes = vec![
            create_process(200, 999, "vim"),  // 999 doesn't exist
        ];

        let tree = ProcessTree::build(processes);

        // Should not crash, should return false
        assert!(!tree.has_ancestor(200, "claude"));
    }

    #[test]
    fn test_has_ancestor_root_process() {
        // Process with ppid 0 (like init)
        let processes = vec![
            create_process(1, 0, "init"),
        ];

        let tree = ProcessTree::build(processes);

        assert!(!tree.has_ancestor(1, "claude"));
        assert!(tree.has_ancestor(1, "init"));
    }

    #[test]
    #[ignore]
    fn test_list_processes() {
        let ds = SystemProcessDataSource::new();
        let processes = ds.list_processes().unwrap();

        // 少なくとも1つのプロセスがあるはず
        assert!(!processes.is_empty());

        // PID 1 (init/launchd) が存在するはず
        let init = processes.iter().find(|p| p.pid == 1);
        assert!(init.is_some());
    }
}
