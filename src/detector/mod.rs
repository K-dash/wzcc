pub mod identify;

pub use identify::ClaudeCodeDetector;

/// Claude Code 検出の根拠
#[derive(Debug, Clone)]
pub enum DetectionReason {
    /// TTY マッチング + プロセス名の直接一致
    DirectTtyMatch { process_name: String },
    /// TTY マッチング + 親プロセスに claude が存在 (wrapper 経由)
    WrapperDetected { wrapper_process: String },
}

impl DetectionReason {
    /// UI 表示用の文字列
    pub fn display(&self) -> String {
        match self {
            DetectionReason::DirectTtyMatch { process_name } => {
                format!("Direct: TTY match ({})", process_name)
            }
            DetectionReason::WrapperDetected { wrapper_process } => {
                format!("Wrapper: parent process found ({})", wrapper_process)
            }
        }
    }
}
