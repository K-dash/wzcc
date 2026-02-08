use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};

/// Get git branch name from a working directory.
pub fn get_git_branch(cwd: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            return Some(branch);
        }
    }
    None
}

/// Cache for git branch lookups with TTL.
pub struct GitBranchCache {
    entries: HashMap<String, (Option<String>, Instant)>,
    ttl: Duration,
}

impl GitBranchCache {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub fn get(&mut self, cwd: &str) -> Option<String> {
        if let Some((branch, fetched_at)) = self.entries.get(cwd) {
            if fetched_at.elapsed() < self.ttl {
                return branch.clone();
            }
        }

        let branch = get_git_branch(cwd);
        self.entries
            .insert(cwd.to_string(), (branch.clone(), Instant::now()));
        branch
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
