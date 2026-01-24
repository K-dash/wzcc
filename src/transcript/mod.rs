//! Transcript file monitoring and parsing for Claude Code session state detection.
//!
//! Claude Code stores conversation transcripts at:
//! `~/.claude/projects/{encoded-cwd}/{session_id}.jsonl`

mod parser;
mod path;
mod state;
mod watcher;

pub use parser::{TranscriptEntry, get_last_assistant_text, get_last_user_prompt};
pub use path::{encode_cwd, get_transcript_dir, get_latest_transcript};
pub use state::{SessionStatus, detect_session_status};
pub use watcher::TranscriptWatcher;
