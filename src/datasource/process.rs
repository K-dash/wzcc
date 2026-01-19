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
        let mut visited = std::collections::HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(pid);

        while let Some(current_pid) = queue.pop_front() {
            if visited.contains(&current_pid) {
                continue;
            }
            visited.insert(current_pid);

            // 現在のプロセスを取得
            let Some(proc) = self.processes.get(&current_pid) else {
                continue;
            };

            // コマンド名または引数に target が含まれるかチェック
            if proc.command.to_lowercase().contains(target) {
                return true;
            }

            if let Some(args) = &proc.args {
                if args.to_lowercase().contains(target) {
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
