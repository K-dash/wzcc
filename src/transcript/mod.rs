//! Transcript file monitoring and parsing for Claude Code session state detection.
//!
//! Claude Code stores conversation transcripts at:
//! `~/.claude/projects/{encoded-cwd}/{session_id}.jsonl`

mod parser;
mod path;
mod state;
mod watcher;

pub use parser::{get_last_assistant_text, get_last_user_prompt, TranscriptEntry};
pub use path::{encode_cwd, get_latest_transcript, get_transcript_dir};
pub use state::{detect_session_status, SessionStatus};
