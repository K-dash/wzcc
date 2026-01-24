use anyhow::Result;
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
};
use std::time::Duration;

/// TUI イベント
#[derive(Debug, Clone)]
pub enum Event {
    /// キー入力
    Key(KeyEvent),
    /// マウス入力
    Mouse(MouseEvent),
    /// Tick (定期更新)
    Tick,
    /// リサイズ
    Resize(u16, u16),
}

/// イベントハンドラ
pub struct EventHandler {
    /// Tick 間隔 (ms)
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate_ms: u64) -> Self {
        Self {
            tick_rate: Duration::from_millis(tick_rate_ms),
        }
    }

    /// 次のイベントを取得
    pub fn next(&self) -> Result<Event> {
        // crossterm の poll でタイムアウト付きでイベントを待つ
        if event::poll(self.tick_rate)? {
            match event::read()? {
                CrosstermEvent::Key(key) => Ok(Event::Key(key)),
                CrosstermEvent::Mouse(mouse) => Ok(Event::Mouse(mouse)),
                CrosstermEvent::Resize(w, h) => Ok(Event::Resize(w, h)),
                _ => Ok(Event::Tick),
            }
        } else {
            // タイムアウト → Tick イベント
            Ok(Event::Tick)
        }
    }
}

/// マウスイベントがシングルクリックかどうか
pub fn is_mouse_click(mouse: &MouseEvent) -> bool {
    matches!(mouse.kind, MouseEventKind::Down(_))
}

/// マウスイベントがダブルクリックかどうか（crossterm はダブルクリックを直接サポートしてないので時間で判定）
pub fn is_mouse_double_click(mouse: &MouseEvent) -> bool {
    // Note: ダブルクリック判定は App 側で時間計測して行う
    matches!(mouse.kind, MouseEventKind::Down(_))
}

/// キーイベントのヘルパー
pub fn is_quit_key(key: &KeyEvent) -> bool {
    matches!(
        key.code,
        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('c')
    ) || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

pub fn is_up_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Up | KeyCode::Char('k'))
}

pub fn is_down_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Down | KeyCode::Char('j'))
}

pub fn is_enter_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Enter)
}

pub fn is_refresh_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('r'))
}
