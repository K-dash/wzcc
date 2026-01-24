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
                let name = Self::basename(process_name);
                format!("Direct: TTY match ({})", name)
            }
            DetectionReason::WrapperDetected { wrapper_process } => {
                let name = Self::basename(wrapper_process);
                format!("Wrapper: parent process ({})", name)
            }
        }
    }

    /// パスから basename を取得（スペースがあれば最初の部分だけ使う）
    fn basename(path: &str) -> &str {
        // まずスペースで分割して最初の部分を取得
        let first_part = path.split_whitespace().next().unwrap_or(path);
        // 次にパスから basename を取得
        first_part.rsplit('/').next().unwrap_or(first_part)
    }
}
