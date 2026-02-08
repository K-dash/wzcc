//! Transcript file monitoring and parsing for Claude Code session state detection.
//!
//! Claude Code stores conversation transcripts at:
//! `~/.claude/projects/{encoded-cwd}/{session_id}.jsonl`

mod info;
mod parser;
mod path;
pub mod session_info;
mod state;
pub mod watcher;

pub use info::{read_transcript_info, TranscriptInfo};
pub use parser::{
    extract_conversation_turns, get_last_assistant_text, get_last_user_prompt, ConversationTurn,
    TranscriptEntry,
};
pub use path::{encode_cwd, get_latest_transcript, get_transcript_dir};
pub use session_info::{detect_session_info, SessionInfo};
pub use state::{detect_session_status, SessionStatus};
pub use watcher::TranscriptWatcher;
