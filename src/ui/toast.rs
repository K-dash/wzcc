use std::time::{Duration, Instant};

/// Toast notification type
#[derive(Debug, Clone)]
pub enum ToastType {
    /// Success (green)
    Success,
    /// Error (red)
    Error,
}

/// Toast notification
#[derive(Debug, Clone)]
pub struct Toast {
    /// Message
    pub message: String,
    /// Type
    pub toast_type: ToastType,
    /// Display start time
    pub start_time: Instant,
    /// Display duration (milliseconds)
    pub duration_ms: u64,
}

impl Toast {
    /// Create success toast
    pub fn success(message: String) -> Self {
        Self {
            message,
            toast_type: ToastType::Success,
            start_time: Instant::now(),
            duration_ms: 1000, // 1 second
        }
    }

    /// Create error toast
    pub fn error(message: String) -> Self {
        Self {
            message,
            toast_type: ToastType::Error,
            start_time: Instant::now(),
            duration_ms: 3000, // 3 seconds
        }
    }

    /// Check if toast has expired
    pub fn is_expired(&self) -> bool {
        self.start_time.elapsed() > Duration::from_millis(self.duration_ms)
    }
}
