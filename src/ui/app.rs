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

/// TUI application
pub struct App {
    /// Claude Code session list
    sessions: Vec<ClaudeSession>,
    /// List selection state
    list_state: ListState,
    /// Data sources
    pane_ds: WeztermDataSource,
    process_ds: SystemProcessDataSource,
    detector: ClaudeCodeDetector,
    /// Dirty flag (needs redraw)
    dirty: bool,
    /// Refreshing flag
    refreshing: bool,
    /// Needs full redraw (to prevent artifacts on selection change)
    needs_full_redraw: bool,
    /// 'g' key pressed state (for gg sequence)
    pending_g: bool,
    /// Previous last_output snapshot (for change detection)
    prev_last_outputs: Vec<Option<String>>,
    /// Last click time and index (for double click detection)
    last_click: Option<(std::time::Instant, usize)>,
    /// List area Rect (for click position calculation)
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

    /// Refresh session list
    pub fn refresh(&mut self) -> Result<()> {
        // Preserve currently selected pane_id
        let selected_pane_id = self
            .list_state
            .selected()
            .and_then(|i| self.sessions.get(i))
            .map(|s| s.pane.pane_id);

        // Get current workspace
        let current_workspace = self.pane_ds.get_current_workspace()?;

        let panes = self.pane_ds.list_panes()?;

        // Build process tree once (optimization)
        let process_tree = self.process_ds.build_tree()?;

        self.sessions = panes
            .into_iter()
            .filter_map(|pane| {
                // Filter by current workspace only
                if pane.workspace != current_workspace {
                    return None;
                }

                // Try to detect Claude Code (reusing process tree)
                let reason = self
                    .detector
                    .detect_by_tty_with_tree(&pane, &process_tree)
                    .ok()??;

                // Get session status
                let (status, last_prompt, last_output) =
                    ClaudeSession::detect_status_and_output(&pane);

                // Get git branch
                let git_branch = pane
                    .cwd_path()
                    .and_then(|cwd| ClaudeSession::get_git_branch(&cwd));

                // Keep only detected sessions
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

        // Cannot show last_output when multiple sessions share the same cwd
        // Count sessions per cwd
        let mut cwd_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for session in &self.sessions {
            if let Some(cwd) = session.pane.cwd_path() {
                *cwd_counts.entry(cwd).or_insert(0) += 1;
            }
        }

        // Clear last_prompt/last_output for sessions with duplicate cwd
        for session in &mut self.sessions {
            if let Some(cwd) = session.pane.cwd_path() {
                if cwd_counts.get(&cwd).copied().unwrap_or(0) > 1 {
                    session.last_prompt = None;
                    session.last_output = Some("Multiple sessions share this CWD 😢".to_string());
                }
            }
        }

        // Group by cwd (sort)
        self.sessions.sort_by(|a, b| {
            let cwd_a = a.pane.cwd_path().unwrap_or_default();
            let cwd_b = b.pane.cwd_path().unwrap_or_default();
            cwd_a.cmp(&cwd_b).then(a.pane.pane_id.cmp(&b.pane.pane_id))
        });

        // Maintain selection position (reselect if same pane_id exists)
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

    /// Select next item
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

    /// Select previous item
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

    /// Select first item (gg)
    pub fn select_first(&mut self) {
        if !self.sessions.is_empty() {
            self.list_state.select(Some(0));
            self.dirty = true;
        }
    }

    /// Select last item (G)
    pub fn select_last(&mut self) {
        if !self.sessions.is_empty() {
            self.list_state.select(Some(self.sessions.len() - 1));
            self.dirty = true;
        }
    }

    /// Jump to selected session
    pub fn jump_to_selected(&mut self) -> Result<()> {
        if let Some(i) = self.list_state.selected() {
            if let Some(session) = self.sessions.get(i) {
                let pane_id = session.pane.pane_id;

                // Activate pane
                WeztermCli::activate_pane(pane_id)?;
            }
        }

        Ok(())
    }

    /// Calculate session index from list display row
    /// Returns the session corresponding to the clicked row, considering group headers
    fn row_to_session_index(&self, row: usize) -> Option<usize> {
        // Map row number to session index
        let mut current_row = 0;
        let mut current_cwd: Option<String> = None;

        for (session_idx, session) in self.sessions.iter().enumerate() {
            let cwd = session.pane.cwd_path().unwrap_or_default();

            // Add header row for new CWD
            if current_cwd.as_ref() != Some(&cwd) {
                current_cwd = Some(cwd.clone());
                // Header row
                if current_row == row {
                    // Ignore header click (not a session)
                    return None;
                }
                current_row += 1;
            }

            // Session row
            if current_row == row {
                return Some(session_idx);
            }
            current_row += 1;
        }

        None
    }

    /// Run TUI
    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Initial refresh
        self.refresh()?;

        // Event handler (auto-update every 3 seconds)
        let event_handler = EventHandler::new(3000);

        // Main loop
        let result = loop {
            // Only draw when dirty flag is set
            if self.dirty {
                // Clear terminal when full redraw is needed
                if self.needs_full_redraw {
                    terminal.clear()?;
                    self.needs_full_redraw = false;
                }
                terminal.draw(|f| self.render(f))?;
                self.dirty = false;
            }

            // Event processing
            match event_handler.next()? {
                Event::Key(key) => {
                    // Handle gg sequence
                    if self.pending_g {
                        self.pending_g = false;
                        if key.code == KeyCode::Char('g') {
                            // gg -> jump to first
                            self.select_first();
                            continue;
                        }
                        // Reset pending if different key comes after g
                    }

                    if is_quit_key(&key) {
                        break Ok(());
                    } else if is_down_key(&key) {
                        self.select_next();
                    } else if is_up_key(&key) {
                        self.select_previous();
                    } else if key.code == KeyCode::Char('g') {
                        // First g -> set pending state
                        self.pending_g = true;
                    } else if key.code == KeyCode::Char('G') {
                        // G -> jump to last
                        self.select_last();
                    } else if is_enter_key(&key) {
                        // Try to jump (TUI continues)
                        let _ = self.jump_to_selected();
                    } else if is_refresh_key(&key) {
                        // Show refreshing indicator then update
                        self.refreshing = true;
                        self.dirty = true;
                        terminal.draw(|f| self.render(f))?;
                        self.refresh()?;
                        self.refreshing = false;
                    }
                }
                Event::Mouse(mouse) => {
                    // Handle left click only
                    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                        // Check if click is inside list area
                        if let Some(area) = self.list_area {
                            if mouse.column >= area.x
                                && mouse.column < area.x + area.width
                                && mouse.row >= area.y
                                && mouse.row < area.y + area.height
                            {
                                // Relative row excluding border and title (first row)
                                let relative_row = mouse.row.saturating_sub(area.y + 1);

                                // Calculate clicked session index
                                if let Some(idx) = self.row_to_session_index(relative_row as usize)
                                {
                                    let now = std::time::Instant::now();

                                    // Double click detection (click same item within 300ms)
                                    let is_double_click = self
                                        .last_click
                                        .map(|(time, last_idx)| {
                                            last_idx == idx
                                                && now.duration_since(time).as_millis() < 300
                                        })
                                        .unwrap_or(false);

                                    if is_double_click {
                                        // Double click -> jump
                                        self.list_state.select(Some(idx));
                                        let _ = self.jump_to_selected();
                                        self.last_click = None;
                                    } else {
                                        // Single click -> select
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
                    // Auto-refresh every 3 seconds (no indicator)
                    self.refresh()?;

                    // Full redraw only when last_output changes (prevent flickering)
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

        // Cleanup terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    /// Render
    fn render(&mut self, f: &mut ratatui::Frame) {
        let size = f.area();

        // TODO: Toast notification deferred for later
        // Skipping due to rendering visibility issue after pane switch
        let main_area = size;

        // 2-column layout (left: list 45%, right: details 55%)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(main_area);

        // Render list (update list_area)
        self.list_area = render_list(
            f,
            chunks[0],
            &self.sessions,
            &mut self.list_state,
            self.refreshing,
        );

        // Render details
        render_details(f, chunks[1], &self.sessions, self.list_state.selected());
    }
}
