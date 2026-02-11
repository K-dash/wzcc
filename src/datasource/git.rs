use std::collections::HashMap;
use std::path::Path;
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

/// Detect the worktree relative path if cwd is inside a linked git worktree.
///
/// Returns `None` for regular repos or main worktrees.
/// Returns `Some("relative/path")` for linked worktrees — the path from the
/// main worktree root to cwd (e.g. `.worktree/feat/shimamura-lambda`).
pub fn get_git_worktree_name(cwd: &str) -> Option<String> {
    // Get git-dir: for linked worktrees this is something like
    // /path/to/main/.git/worktrees/<name>
    let git_dir_output = Command::new("git")
        .args(["-C", cwd, "rev-parse", "--git-dir"])
        .output()
        .ok()?;

    if !git_dir_output.status.success() {
        return None;
    }

    let git_dir = String::from_utf8_lossy(&git_dir_output.stdout)
        .trim()
        .to_string();

    // Linked worktrees have a git-dir path containing "/worktrees/"
    if !git_dir.contains("/worktrees/") {
        return None;
    }

    // Get the main worktree root via git-common-dir
    let common_dir_output = Command::new("git")
        .args(["-C", cwd, "rev-parse", "--git-common-dir"])
        .output()
        .ok()?;

    if !common_dir_output.status.success() {
        return None;
    }

    let common_dir = String::from_utf8_lossy(&common_dir_output.stdout)
        .trim()
        .to_string();

    // common_dir is the .git directory of the main worktree.
    // The main worktree root is its parent.
    let main_root = Path::new(&common_dir).parent()?;

    let cwd_path = Path::new(cwd).canonicalize().ok()?;
    let main_root = main_root.canonicalize().ok()?;

    // Compute relative path from main worktree root to cwd
    cwd_path
        .strip_prefix(&main_root)
        .ok()
        .map(|rel| rel.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
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

/// Cache for git worktree lookups with TTL.
pub struct GitWorktreeCache {
    entries: HashMap<String, (Option<String>, Instant)>,
    ttl: Duration,
}

impl GitWorktreeCache {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub fn get(&mut self, cwd: &str) -> Option<String> {
        if let Some((worktree, fetched_at)) = self.entries.get(cwd) {
            if fetched_at.elapsed() < self.ttl {
                return worktree.clone();
            }
        }

        let worktree = get_git_worktree_name(cwd);
        self.entries
            .insert(cwd.to_string(), (worktree.clone(), Instant::now()));
        worktree
    }
}
