use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Command and arguments to spawn in a new pane.
    /// Default: ["claude"]
    /// Example: ["claude", "--dangerously-skip-permissions"]
    pub spawn_command: Option<Vec<String>>,
}

impl Config {
    /// Load configuration from ~/.config/wzcc/config.toml
    ///
    /// - File missing: returns default config (Ok)
    /// - File exists but invalid TOML: returns Err so caller can show warning
    /// - Field missing or empty array: uses default ["claude"]
    pub fn load() -> Result<Self> {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Ok(Self::default()),
        };

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    /// Returns (program, args) for spawning a new pane.
    /// - None or empty array → ("claude", [])
    /// - ["prog", "arg1", ...] → ("prog", ["arg1", ...])
    pub fn spawn_program_and_args(&self) -> (&str, &[String]) {
        match &self.spawn_command {
            Some(cmd) if !cmd.is_empty() && !cmd[0].trim().is_empty() => (&cmd[0], &cmd[1..]),
            _ => ("claude", &[]),
        }
    }

    fn config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|d| d.join(".config").join("wzcc").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_missing_file() {
        // Config::load should return defaults when file doesn't exist
        // We can't easily test this without mocking the path, but we can test
        // that default config has correct spawn_program_and_args
        let config = Config::default();
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "claude");
        assert!(args.is_empty());
    }

    #[test]
    fn test_load_valid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut file = fs::File::create(&path).unwrap();
        writeln!(file, r#"spawn_command = ["claude", "--flag"]"#).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let config: Config = toml::from_str(&content).unwrap();
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "claude");
        assert_eq!(args, &["--flag".to_string()]);
    }

    #[test]
    fn test_load_invalid_toml() {
        let invalid = "spawn_command = [[[invalid";
        let result: std::result::Result<Config, _> = toml::from_str(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_spawn_program_and_args_default() {
        let config = Config {
            spawn_command: None,
        };
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "claude");
        assert!(args.is_empty());
    }

    #[test]
    fn test_spawn_program_and_args_with_args() {
        let config = Config {
            spawn_command: Some(vec![
                "claude".to_string(),
                "--dangerously-skip-permissions".to_string(),
            ]),
        };
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "claude");
        assert_eq!(args, &["--dangerously-skip-permissions".to_string()]);
    }

    #[test]
    fn test_spawn_program_and_args_empty_array() {
        let config = Config {
            spawn_command: Some(vec![]),
        };
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "claude");
        assert!(args.is_empty());
    }

    #[test]
    fn test_spawn_program_and_args_empty_string() {
        let config = Config {
            spawn_command: Some(vec!["".to_string()]),
        };
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "claude");
        assert!(args.is_empty());
    }

    #[test]
    fn test_spawn_program_and_args_whitespace_only() {
        let config = Config {
            spawn_command: Some(vec!["  ".to_string()]),
        };
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "claude");
        assert!(args.is_empty());
    }

    #[test]
    fn test_spawn_program_and_args_custom_wrapper() {
        let config = Config {
            spawn_command: Some(vec![
                "my-wrapper".to_string(),
                "--profile".to_string(),
                "dev".to_string(),
            ]),
        };
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "my-wrapper");
        assert_eq!(args, &["--profile".to_string(), "dev".to_string()]);
    }

    #[test]
    fn test_load_toml_missing_field() {
        // TOML with no spawn_command field should use defaults
        let content = "# empty config\n";
        let config: Config = toml::from_str(content).unwrap();
        let (prog, args) = config.spawn_program_and_args();
        assert_eq!(prog, "claude");
        assert!(args.is_empty());
    }
}
