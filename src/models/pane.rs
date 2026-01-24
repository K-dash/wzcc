use serde::{Deserialize, Serialize};

/// Wezterm pane information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub pane_id: u32,
    pub tab_id: u32,
    pub window_id: u32,
    pub workspace: String,
    pub title: String,
    pub cwd: Option<String>,
    pub tty_name: Option<String>,
    pub is_active: bool,
    pub tab_title: Option<String>,
    pub window_title: Option<String>,
}

impl Pane {
    /// Remove `file://` prefix from CWD
    pub fn cwd_path(&self) -> Option<String> {
        self.cwd
            .as_ref()
            .and_then(|cwd| cwd.strip_prefix("file://").map(|s| s.to_string()))
    }

    /// Remove `/dev/` prefix from TTY name (for matching with ps aux)
    pub fn tty_short(&self) -> Option<String> {
        self.tty_name
            .as_ref()
            .and_then(|tty| tty.strip_prefix("/dev/").map(|s| s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cwd_path() {
        let pane = Pane {
            pane_id: 0,
            tab_id: 0,
            window_id: 0,
            workspace: "default".to_string(),
            title: "test".to_string(),
            cwd: Some("file:///Users/test/project".to_string()),
            tty_name: None,
            is_active: false,
            tab_title: None,
            window_title: None,
        };

        assert_eq!(pane.cwd_path(), Some("/Users/test/project".to_string()));
    }

    #[test]
    fn test_tty_short() {
        let pane = Pane {
            pane_id: 0,
            tab_id: 0,
            window_id: 0,
            workspace: "default".to_string(),
            title: "test".to_string(),
            cwd: None,
            tty_name: Some("/dev/ttys003".to_string()),
            is_active: false,
            tab_title: None,
            window_title: None,
        };

        assert_eq!(pane.tty_short(), Some("ttys003".to_string()));
    }
}
