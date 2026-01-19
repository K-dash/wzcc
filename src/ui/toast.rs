use std::time::{Duration, Instant};

/// トースト通知の種類
#[derive(Debug, Clone)]
pub enum ToastType {
    /// 成功 (緑)
    Success,
    /// エラー (赤)
    Error,
}

/// トースト通知
#[derive(Debug, Clone)]
pub struct Toast {
    /// メッセージ
    pub message: String,
    /// 種類
    pub toast_type: ToastType,
    /// 表示開始時刻
    pub start_time: Instant,
    /// 表示時間 (ミリ秒)
    pub duration_ms: u64,
}

impl Toast {
    /// 成功トーストを作成
    pub fn success(message: String) -> Self {
        Self {
            message,
            toast_type: ToastType::Success,
            start_time: Instant::now(),
            duration_ms: 1000, // 1秒
        }
    }

    /// エラートーストを作成
    pub fn error(message: String) -> Self {
        Self {
            message,
            toast_type: ToastType::Error,
            start_time: Instant::now(),
            duration_ms: 3000, // 3秒
        }
    }

    /// トーストが表示期限切れかどうか
    pub fn is_expired(&self) -> bool {
        self.start_time.elapsed() > Duration::from_millis(self.duration_ms)
    }
}
