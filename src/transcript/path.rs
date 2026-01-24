//! Transcript file path utilities.

use anyhow::Result;
use std::path::PathBuf;

/// Encode a cwd path to the format used by Claude Code.
/// Replaces `/`, `.`, and `_` with `-`.
///
/// # Example
/// ```
/// use wzcc::transcript::encode_cwd;
/// assert_eq!(encode_cwd("/Users/furukawa/hobby/wzcc"), "-Users-furukawa-hobby-wzcc");
/// assert_eq!(encode_cwd("/Users/furukawa/develop/rcmr_stadium"), "-Users-furukawa-develop-rcmr-stadium");
/// ```
pub fn encode_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| {
            if c == '/' || c == '.' || c == '_' {
                '-'
            } else {
                c
            }
        })
        .collect()
}

/// Get the transcript directory for a given cwd.
/// Returns `~/.claude/projects/{encoded-cwd}/`
pub fn get_transcript_dir(cwd: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let encoded = encode_cwd(cwd);
    Some(home.join(".claude").join("projects").join(encoded))
}

/// Get the latest (most recently modified) transcript file in a directory.
/// Returns the path to the .jsonl file with the newest modification time.
pub fn get_latest_transcript(dir: &PathBuf) -> Result<Option<PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }

    let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only consider .jsonl files
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified = metadata.modified()?;

        match &latest {
            None => latest = Some((path, modified)),
            Some((_, latest_time)) if modified > *latest_time => {
                latest = Some((path, modified));
            }
            _ => {}
        }
    }

    Ok(latest.map(|(path, _)| path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_cwd() {
        assert_eq!(
            encode_cwd("/Users/furukawa/hobby/wzcc"),
            "-Users-furukawa-hobby-wzcc"
        );
        assert_eq!(
            encode_cwd("/Users/furukawa/.claude"),
            "-Users-furukawa--claude"
        );
        assert_eq!(
            encode_cwd("/Users/test/project.name"),
            "-Users-test-project-name"
        );
        // underscore should also be replaced
        assert_eq!(
            encode_cwd("/Users/furukawa/develop/rcmr_stadium"),
            "-Users-furukawa-develop-rcmr-stadium"
        );
    }

    #[test]
    fn test_get_transcript_dir() {
        let dir = get_transcript_dir("/Users/furukawa/hobby/wzcc");
        assert!(dir.is_some());
        let dir = dir.unwrap();
        assert!(dir.ends_with(".claude/projects/-Users-furukawa-hobby-wzcc"));
    }
}
