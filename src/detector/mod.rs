pub mod identify;

pub use identify::ClaudeCodeDetector;

/// Detection reason for Claude Code
#[derive(Debug, Clone)]
pub enum DetectionReason {
    /// TTY matching + direct process name match
    DirectTtyMatch { process_name: String },
    /// TTY matching + claude exists in parent process (via wrapper)
    WrapperDetected { wrapper_process: String },
}

impl DetectionReason {
    /// String for UI display
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

    /// Get basename from path (use only first part if spaces exist)
    fn basename(path: &str) -> &str {
        // First split by whitespace and get the first part
        let first_part = path.split_whitespace().next().unwrap_or(path);
        // Then get basename from path
        first_part.rsplit('/').next().unwrap_or(first_part)
    }
}
