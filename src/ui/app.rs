use crate::cli::WeztermCli;
use crate::datasource::{
    PaneDataSource, ProcessDataSource, SystemProcessDataSource, WeztermDataSource,
};
use crate::detector::ClaudeCodeDetector;
use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::ListState,
    Terminal,
};
use std::io;

use super::event::{
    is_down_key, is_enter_key, is_quit_key, is_refresh_key, is_up_key, Event, EventHandler,
};
use super::render::{render_details, render_list};
use super::session::ClaudeSession;

/// TUI アプリケーション
pub struct App {
    /// Claude Code セッション一覧
    sessions: Vec<ClaudeSession>,
    /// リスト選択状態
    list_state: ListState,
    /// データソース
    pane_ds: WeztermDataSource,
    process_ds: SystemProcessDataSource,
    detector: ClaudeCodeDetector,
    /// dirty flag (再描画が必要か)
    dirty: bool,
    /// リフレッシュ中フラグ
    refreshing: bool,
    /// フル再描画が必要か（選択変更時などに差分描画の残像を防ぐ）
    needs_full_redraw: bool,
    /// 'g' キーが押された状態（gg シーケンス用）
    pending_g: bool,
    /// 前回の last_output のスナップショット（変更検出用）
    prev_last_outputs: Vec<Option<String>>,
    /// 最後のクリック時刻とインデックス（ダブルクリック判定用）
    last_click: Option<(std::time::Instant, usize)>,
    /// リストエリアの Rect（クリック位置計算用）
    list_area: Option<Rect>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            sessions: Vec::new(),
            list_state,
            pane_ds: WeztermDataSource::new(),
            process_ds: SystemProcessDataSource::new(),
            detector: ClaudeCodeDetector::new(),
            dirty: true,
            refreshing: false,
            needs_full_redraw: true,
            pending_g: false,
            prev_last_outputs: Vec::new(),
            last_click: None,
            list_area: None,
        }
    }

    /// セッション一覧をリフレッシュ
    pub fn refresh(&mut self) -> Result<()> {
        // 現在選択中の pane_id を保持
        let selected_pane_id = self
            .list_state
            .selected()
            .and_then(|i| self.sessions.get(i))
            .map(|s| s.pane.pane_id);

        // 現在の workspace を取得
        let current_workspace = self.pane_ds.get_current_workspace()?;

        let panes = self.pane_ds.list_panes()?;

        // プロセスツリーを1回だけ構築（最適化）
        let process_tree = self.process_ds.build_tree()?;

        self.sessions = panes
            .into_iter()
            .filter_map(|pane| {
                // 現在の workspace のみフィルタリング
                if pane.workspace != current_workspace {
                    return None;
                }

                // Claude Code 検出を試みる（プロセスツリーを再利用）
                let reason = self
                    .detector
                    .detect_by_tty_with_tree(&pane, &process_tree)
                    .ok()??;

                // セッション状態を取得
                let (status, last_prompt, last_output) =
                    ClaudeSession::detect_status_and_output(&pane);

                // Git branch を取得
                let git_branch = pane.cwd_path().and_then(|cwd| ClaudeSession::get_git_branch(&cwd));

                // 検出されたセッションのみ保持
                Some(ClaudeSession {
                    pane,
                    detected: true,
                    reason,
                    status,
                    git_branch,
                    last_prompt,
                    last_output,
                })
            })
            .collect();

        // 同じ cwd で複数セッションがある場合は last_output を表示できない
        // cwd ごとのセッション数をカウント
        let mut cwd_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for session in &self.sessions {
            if let Some(cwd) = session.pane.cwd_path() {
                *cwd_counts.entry(cwd).or_insert(0) += 1;
            }
        }

        // 重複している cwd のセッションは last_prompt/last_output をクリア
        for session in &mut self.sessions {
            if let Some(cwd) = session.pane.cwd_path() {
                if cwd_counts.get(&cwd).copied().unwrap_or(0) > 1 {
                    session.last_prompt = None;
                    session.last_output = Some("Multiple sessions share this CWD 😢".to_string());
                }
            }
        }

        // cwd でグループ化（ソート）
        self.sessions.sort_by(|a, b| {
            let cwd_a = a.pane.cwd_path().unwrap_or_default();
            let cwd_b = b.pane.cwd_path().unwrap_or_default();
            cwd_a.cmp(&cwd_b).then(a.pane.pane_id.cmp(&b.pane.pane_id))
        });

        // 選択位置を維持（同じ pane_id があれば選択し直す）
        if !self.sessions.is_empty() {
            let new_index = selected_pane_id
                .and_then(|id| self.sessions.iter().position(|s| s.pane.pane_id == id))
                .unwrap_or(0);
            self.list_state.select(Some(new_index));
        } else {
            self.list_state.select(None);
        }

        self.dirty = true;

        Ok(())
    }

    /// 次のアイテムを選択
    pub fn select_next(&mut self) {
        if self.sessions.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.sessions.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };

        self.list_state.select(Some(i));
        self.dirty = true;
    }

    /// 前のアイテムを選択
    pub fn select_previous(&mut self) {
        if self.sessions.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.sessions.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };

        self.list_state.select(Some(i));
        self.dirty = true;
    }

    /// 先頭のアイテムを選択 (gg)
    pub fn select_first(&mut self) {
        if !self.sessions.is_empty() {
            self.list_state.select(Some(0));
            self.dirty = true;
        }
    }

    /// 末尾のアイテムを選択 (G)
    pub fn select_last(&mut self) {
        if !self.sessions.is_empty() {
            self.list_state.select(Some(self.sessions.len() - 1));
            self.dirty = true;
        }
    }

    /// 選択中のセッションにジャンプ
    pub fn jump_to_selected(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected() {
            if let Some(session) = self.sessions.get(i) {
                let pane_id = session.pane.pane_id;

                // Pane をアクティベート
                WeztermCli::activate_pane(pane_id)?;
            }
        }

        Ok(())
    }

    /// リスト表示行からセッションインデックスを計算
    /// グループヘッダーを考慮して、クリックされた行が対応するセッションを返す
    fn row_to_session_index(&self, row: usize) -> Option<usize> {
        // 行番号からセッションインデックスをマッピング
        let mut current_row = 0;
        let mut current_cwd: Option<String> = None;

        for (session_idx, session) in self.sessions.iter().enumerate() {
            let cwd = session.pane.cwd_path().unwrap_or_default();

            // 新しい CWD の場合はヘッダー行を追加
            if current_cwd.as_ref() != Some(&cwd) {
                current_cwd = Some(cwd.clone());
                // ヘッダー行
                if current_row == row {
                    // ヘッダークリックは無視（セッションじゃない）
                    return None;
                }
                current_row += 1;
            }

            // セッション行
            if current_row == row {
                return Some(session_idx);
            }
            current_row += 1;
        }

        None
    }

    /// TUI を実行
    pub fn run(&mut self) -> Result<()> {
        // ターミナルをセットアップ
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // 初回リフレッシュ
        self.refresh()?;

        // イベントハンドラ (3秒ごとに自動更新)
        let event_handler = EventHandler::new(3000);

        // メインループ
        let result = loop {
            // dirty flag が立っている場合のみ描画
            if self.dirty {
                // フル再描画が必要な場合はターミナルをクリア
                if self.needs_full_redraw {
                    terminal.clear()?;
                    self.needs_full_redraw = false;
                }
                terminal.draw(|f| self.render(f))?;
                self.dirty = false;
            }

            // イベント処理
            match event_handler.next()? {
                Event::Key(key) => {
                    // gg シーケンスの処理
                    if self.pending_g {
                        self.pending_g = false;
                        if key.code == KeyCode::Char('g') {
                            // gg → 先頭へ
                            self.select_first();
                            continue;
                        }
                        // g の後に別のキーが来たら pending をリセットして通常処理
                    }

                    if is_quit_key(&key) {
                        break Ok(());
                    } else if is_down_key(&key) {
                        self.select_next();
                    } else if is_up_key(&key) {
                        self.select_previous();
                    } else if key.code == KeyCode::Char('g') {
                        // 最初の g → pending 状態に
                        self.pending_g = true;
                    } else if key.code == KeyCode::Char('G') {
                        // G → 末尾へ
                        self.select_last();
                    } else if is_enter_key(&key) {
                        // ジャンプを試みる（TUI は継続）
                        let _ = self.jump_to_selected();
                    } else if is_refresh_key(&key) {
                        // リフレッシュ中表示を出してから更新
                        self.refreshing = true;
                        self.dirty = true;
                        terminal.draw(|f| self.render(f))?;
                        self.refresh()?;
                        self.refreshing = false;
                    }
                }
                Event::Mouse(mouse) => {
                    // 左クリックのみ処理
                    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                        // リストエリア内のクリックかチェック
                        if let Some(area) = self.list_area {
                            if mouse.column >= area.x
                                && mouse.column < area.x + area.width
                                && mouse.row >= area.y
                                && mouse.row < area.y + area.height
                            {
                                // ボーダーとタイトル（1行目）を除いた相対行
                                let relative_row = mouse.row.saturating_sub(area.y + 1);

                                // クリックされたセッションインデックスを計算
                                if let Some(idx) = self.row_to_session_index(relative_row as usize)
                                {
                                    let now = std::time::Instant::now();

                                    // ダブルクリック判定（300ms以内に同じアイテムをクリック）
                                    let is_double_click = self
                                        .last_click
                                        .map(|(time, last_idx)| {
                                            last_idx == idx
                                                && now.duration_since(time).as_millis() < 300
                                        })
                                        .unwrap_or(false);

                                    if is_double_click {
                                        // ダブルクリック → ジャンプ
                                        self.list_state.select(Some(idx));
                                        let _ = self.jump_to_selected();
                                        self.last_click = None;
                                    } else {
                                        // シングルクリック → 選択
                                        self.list_state.select(Some(idx));
                                        self.dirty = true;
                                        self.last_click = Some((now, idx));
                                    }
                                }
                            }
                        }
                    }
                }
                Event::Resize(_, _) => {
                    self.dirty = true;
                }
                Event::Tick => {
                    // 3秒ごとに自動リフレッシュ（インジケータなし）
                    self.refresh()?;

                    // last_output が変わった場合のみフル再描画（チラつき防止）
                    let current_outputs: Vec<Option<String>> = self
                        .sessions
                        .iter()
                        .map(|s| s.last_output.clone())
                        .collect();

                    if current_outputs != self.prev_last_outputs {
                        self.needs_full_redraw = true;
                        self.prev_last_outputs = current_outputs;
                    }
                }
            }
        };

        // ターミナルをクリーンアップ
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    /// 描画
    fn render(&mut self, f: &mut ratatui::Frame) {
        let size = f.area();

        // TODO: トースト通知は Phase 4 で保留中
        // pane 切り替え後に描画が見えない問題があるため一旦スキップ
        let main_area = size;

        // 2カラムレイアウト (左: リスト 45%, 右: 詳細 55%)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(main_area);

        // リスト描画（list_area を更新）
        self.list_area = render_list(
            f,
            chunks[0],
            &self.sessions,
            &mut self.list_state,
            self.refreshing,
        );

        // 詳細描画
        render_details(f, chunks[1], &self.sessions, self.list_state.selected());
    }
}
