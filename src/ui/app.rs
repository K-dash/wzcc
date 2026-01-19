use crate::cli::WeztermCli;
use crate::datasource::{PaneDataSource, SystemProcessDataSource, WeztermDataSource};
use crate::detector::ClaudeCodeDetector;
use crate::models::Pane;
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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
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
        }
    }

    /// セッション一覧をリフレッシュ
    pub fn refresh(&mut self) -> Result<()> {
        // 現在の workspace を取得
        let current_workspace = self.pane_ds.get_current_workspace()?;

        let panes = self.pane_ds.list_panes()?;

        self.sessions = panes
            .into_iter()
            .filter_map(|pane| {
                // 現在の workspace のみフィルタリング
                if pane.workspace != current_workspace {
                    return None;
                }

                // Claude Code 検出を試みる
                let detected = self.detector.detect_by_tty(&pane, &self.process_ds).ok()?;

                // 検出されたセッションのみ保持
                if detected {
                    Some(ClaudeSession { pane, detected })
                } else {
                    None
                }
            })
            .collect();

        // 選択をリセット
        if !self.sessions.is_empty() {
            self.list_state.select(Some(0));
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

    /// 選択中のセッションにジャンプ
    pub fn jump_to_selected(&self) -> Result<()> {
        if let Some(i) = self.list_state.selected() {
            if let Some(session) = self.sessions.get(i) {
                // Pane をアクティベート (workspace も自動で切り替わるはず)
                // tab のアクティベートは不要 (pane のアクティベートで自動的に tab も切り替わる)
                WeztermCli::activate_pane(session.pane.pane_id)?;
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

        // イベントハンドラ (50ms tick)
        let event_handler = EventHandler::new(50);

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
                        if let Err(e) = self.jump_to_selected() {
                            // ジャンプ失敗は無視 (ログに残す余地はある)
                            eprintln!("Jump failed: {}", e);
                        } else {
                            // ジャンプ成功したら TUI を終了
                            break Ok(());
                        }
                    } else if is_refresh_key(&key) {
                        self.refresh()?;
                    }
                }
                Event::Resize(_, _) => {
                    self.dirty = true;
                }
                Event::Tick => {
                    // Tick では何もしない (将来的に自動リフレッシュを入れる場合はここ)
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

        // 2カラムレイアウト (左: リスト 80%, 右: 詳細 20%)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(80), Constraint::Percentage(20)])
            .split(size);

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

                // アイコン
                let icon = if pane.title.contains('✳') || pane.title.contains('✶') {
                    "✳"
                } else {
                    "●"
                };

                // タイトル (最大50文字)
                let title = if pane.title.len() > 50 {
                    format!("{}...", &pane.title[..47])
                } else {
                    pane.title.clone()
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("{} ", icon),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("Pane {}: ", pane.pane_id),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::raw(title),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Claude Code Sessions ({}) ", self.sessions.len())),
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
                    Line::from(vec![
                        Span::styled("Tab: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(pane.tab_id.to_string()),
                    ]),
                    Line::from(vec![
                        Span::styled("Window: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(pane.window_id.to_string()),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Workspace: ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(&pane.workspace),
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

                lines
            } else {
                vec![Line::from("No selection")]
            }
        } else {
            vec![Line::from("No sessions")]
        };

        let paragraph =
            Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Details "));

        f.render_widget(paragraph, area);
    }
}
