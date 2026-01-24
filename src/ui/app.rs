use crate::cli::WeztermCli;
use crate::datasource::{PaneDataSource, ProcessDataSource, SystemProcessDataSource, WeztermDataSource};
use crate::detector::{ClaudeCodeDetector, DetectionReason};
use crate::models::Pane;
use crate::transcript::{get_transcript_dir, get_latest_transcript, detect_session_status, SessionStatus};
use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io;

use super::event::{
    is_down_key, is_enter_key, is_quit_key, is_refresh_key, is_up_key, Event, EventHandler,
};

/// Claude Code セッション情報
#[derive(Debug, Clone)]
pub struct ClaudeSession {
    pub pane: Pane,
    pub detected: bool,
    #[allow(dead_code)]
    pub reason: DetectionReason,
    /// Session status (Processing/Idle/WaitingForUser/Unknown)
    pub status: SessionStatus,
    /// Git branch name
    pub git_branch: Option<String>,
}

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
                let reason = self.detector.detect_by_tty_with_tree(&pane, &process_tree).ok()??;

                // セッション状態を取得
                let status = Self::detect_status_for_pane(&pane);

                // Git branch を取得
                let git_branch = pane.cwd_path().and_then(|cwd| Self::get_git_branch(&cwd));

                // 検出されたセッションのみ保持
                Some(ClaudeSession {
                    pane,
                    detected: true,
                    reason,
                    status,
                    git_branch,
                })
            })
            .collect();

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

    /// Pane の cwd からトランスクリプトを読んで状態を検出
    fn detect_status_for_pane(pane: &Pane) -> SessionStatus {
        let cwd = match pane.cwd_path() {
            Some(cwd) => cwd,
            None => return SessionStatus::Unknown,
        };

        let dir = match get_transcript_dir(&cwd) {
            Some(dir) => dir,
            None => return SessionStatus::Unknown,
        };

        let transcript_path = match get_latest_transcript(&dir) {
            Ok(Some(path)) => path,
            _ => return SessionStatus::Unknown,
        };

        detect_session_status(&transcript_path).unwrap_or(SessionStatus::Unknown)
    }

    /// cwd から git branch を取得
    fn get_git_branch(cwd: &str) -> Option<String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !branch.is_empty() {
                return Some(branch);
            }
        }
        None
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

    /// TUI を実行
    pub fn run(&mut self) -> Result<()> {
        // ターミナルをセットアップ
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
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
                terminal.draw(|f| self.render(f))?;
                self.dirty = false;
            }

            // イベント処理
            match event_handler.next()? {
                Event::Key(key) => {
                    if is_quit_key(&key) {
                        break Ok(());
                    } else if is_down_key(&key) {
                        self.select_next();
                    } else if is_up_key(&key) {
                        self.select_previous();
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
                Event::Resize(_, _) => {
                    self.dirty = true;
                }
                Event::Tick => {
                    // 3秒ごとに自動リフレッシュ（インジケータなし）
                    self.refresh()?;
                }
            }
        };

        // ターミナルをクリーンアップ
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    /// 描画
    fn render(&mut self, f: &mut ratatui::Frame) {
        let size = f.area();

        // TODO: トースト通知は Phase 4 で保留中
        // pane 切り替え後に描画が見えない問題があるため一旦スキップ
        let main_area = size;

        // 2カラムレイアウト (左: リスト 40%, 右: 詳細 60%)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(main_area);

        self.render_list(f, chunks[0]);
        self.render_details(f, chunks[1]);
    }

    /// リスト描画
    fn render_list(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .sessions
            .iter()
            .map(|session| {
                let pane = &session.pane;

                // 状態アイコンと色
                let (status_icon, status_color) = match &session.status {
                    SessionStatus::Processing => ("●", Color::Yellow),
                    SessionStatus::Idle => ("○", Color::Green),
                    SessionStatus::WaitingForUser { .. } => ("◐", Color::Magenta),
                    SessionStatus::Unknown => ("?", Color::DarkGray),
                };

                // タイトル (最大40文字)
                let title = if pane.title.len() > 40 {
                    format!("{}...", &pane.title[..37])
                } else {
                    pane.title.clone()
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("{} ", status_icon),
                        Style::default()
                            .fg(status_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("Pane {}: ", pane.pane_id),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::raw(title),
                    Span::styled(
                        format!(" [{}]", session.status.as_str()),
                        Style::default().fg(status_color),
                    ),
                ]);

                ListItem::new(line)
            })
            .collect();

        // タイトル（リフレッシュ中はインジケータ表示）
        let title = if self.refreshing {
            " ⌛ Claude Code Sessions - Refreshing... ".to_string()
        } else {
            format!(" Claude Code Sessions ({}) ", self.sessions.len())
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// 詳細描画
    fn render_details(&self, f: &mut ratatui::Frame, area: Rect) {
        let text = if let Some(i) = self.list_state.selected() {
            if let Some(session) = self.sessions.get(i) {
                let pane = &session.pane;

                let mut lines = vec![
                    Line::from(vec![
                        Span::styled("Pane: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(pane.pane_id.to_string()),
                    ]),
                ];

                if let Some(cwd) = pane.cwd_path() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "CWD:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )]));
                    lines.push(Line::from(cwd));
                }

                if let Some(tty) = &pane.tty_name {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("TTY: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(tty),
                    ]));
                }

                // Phase 3: セッション状態を表示
                lines.push(Line::from(""));
                let (status_color, status_text) = match &session.status {
                    SessionStatus::Processing => (Color::Yellow, "Processing"),
                    SessionStatus::Idle => (Color::Green, "Idle"),
                    SessionStatus::WaitingForUser { tools } => {
                        let tools_str = if tools.is_empty() {
                            "Approval".to_string()
                        } else {
                            format!("Approval ({})", tools.join(", "))
                        };
                        (Color::Magenta, Box::leak(tools_str.into_boxed_str()) as &str)
                    }
                    SessionStatus::Unknown => (Color::DarkGray, "Unknown"),
                };
                lines.push(Line::from(vec![
                    Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(status_text, Style::default().fg(status_color)),
                ]));

                // Git branch を表示
                if let Some(branch) = &session.git_branch {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("Branch: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(branch, Style::default().fg(Color::Cyan)),
                    ]));
                }

                lines
            } else {
                vec![Line::from("No selection")]
            }
        } else {
            vec![Line::from("No sessions")]
        };

        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Details "))
            .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }

    // TODO: トースト描画は Phase 4 で保留中
    // pane 切り替え後に描画が見えない問題があるため一旦スキップ
    /*
    fn render_toast(&self, f: &mut ratatui::Frame, area: Rect) {
        if let Some(toast) = &self.toast {
            use super::toast::ToastType;

            let (color, symbol) = match toast.toast_type {
                ToastType::Success => (Color::Green, "✓"),
                ToastType::Error => (Color::Red, "✗"),
            };

            let text = vec![Line::from(vec![
                Span::styled(symbol, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::raw(&toast.message),
            ])];

            let paragraph = Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default().fg(color));

            f.render_widget(paragraph, area);
        }
    }
    */
}
