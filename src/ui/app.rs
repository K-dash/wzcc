use crate::cli::{switch_workspace, WeztermCli};
use crate::datasource::{
    PaneDataSource, ProcessDataSource, SystemProcessDataSource, WeztermDataSource,
};
use crate::detector::ClaudeCodeDetector;
use crate::session_mapping::SessionMapping;
use anyhow::Result;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, KeyCode, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::ListState,
    Terminal,
};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use super::event::{
    is_down_key, is_enter_key, is_quit_key, is_refresh_key, is_up_key, Event, EventHandler,
};
use super::input_buffer::InputBuffer;
use super::render::{render_details, render_footer, render_list};
use super::session::ClaudeSession;
use super::toast::Toast;

/// Cache for git branch lookups with TTL
struct GitBranchCache {
    entries: HashMap<String, (Option<String>, Instant)>,
    ttl: Duration,
}

impl GitBranchCache {
    fn new(ttl_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    fn get(&mut self, cwd: &str) -> Option<String> {
        if let Some((branch, fetched_at)) = self.entries.get(cwd) {
            if fetched_at.elapsed() < self.ttl {
                return branch.clone();
            }
        }

        let branch = ClaudeSession::get_git_branch(cwd);
        self.entries
            .insert(cwd.to_string(), (branch.clone(), Instant::now()));
        branch
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

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
    /// File watcher for transcript changes
    _watcher: Option<RecommendedWatcher>,
    /// Receiver for file change events
    file_change_rx: Option<Receiver<PathBuf>>,
    /// Currently watched directories
    watched_dirs: Vec<PathBuf>,
    /// Animation frame counter for Processing status indicator (0-3)
    animation_frame: u8,
    /// Current workspace name (for detecting cross-workspace jumps)
    current_workspace: String,
    /// Details panel width percentage (default: 45, range: 20-80)
    details_width_percent: u16,
    /// Input mode (for sending prompts to sessions)
    input_mode: bool,
    /// Input buffer with cursor management
    input_buffer: InputBuffer,
    /// Toast notification
    toast: Option<Toast>,
    /// Git branch cache (30s TTL)
    git_branch_cache: GitBranchCache,
    /// Last time a transcript-only refresh was performed (for debouncing)
    last_transcript_refresh: Instant,
    /// Whether a transcript refresh is pending (trailing-edge debounce)
    pending_transcript_refresh: bool,
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
            _watcher: None,
            file_change_rx: None,
            watched_dirs: Vec::new(),
            animation_frame: 0,
            current_workspace: String::new(),
            details_width_percent: 45,
            input_mode: false,
            input_buffer: InputBuffer::new(),
            toast: None,
            git_branch_cache: GitBranchCache::new(30),
            last_transcript_refresh: Instant::now(),
            pending_transcript_refresh: false,
        }
    }

    /// Setup file watcher for transcript directories
    fn setup_watcher(&mut self) -> Result<()> {
        let (tx, rx) = channel::<PathBuf>();

        let watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    // Only handle modify events for .jsonl files
                    if event.kind.is_modify() {
                        for path in event.paths {
                            if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                                let _ = tx.send(path);
                            }
                        }
                    }
                }
            },
            Config::default(),
        )?;

        self.file_change_rx = Some(rx);
        self._watcher = Some(watcher);

        Ok(())
    }

    /// Clean up session mapping files for TTYs that no longer exist.
    ///
    /// This is called at startup to remove stale mappings from previous
    /// WezTerm sessions. Only removes mappings for TTYs that are definitely
    /// not in use by any current pane.
    fn cleanup_inactive_session_mappings(&self) {
        // Get list of all current TTYs from WezTerm
        let active_ttys: Vec<String> = match self.pane_ds.list_panes() {
            Ok(panes) => panes.iter().filter_map(|p| p.tty_short()).collect(),
            Err(_) => return, // If we can't list panes, don't clean up anything
        };

        // Clean up mappings for inactive TTYs
        SessionMapping::cleanup_inactive_ttys(&active_ttys);
    }

    /// Update watched directories based on current sessions
    fn update_watched_dirs(&mut self) -> Result<()> {
        use crate::transcript::get_transcript_dir;

        let mut new_dirs: Vec<PathBuf> = Vec::new();

        for session in &self.sessions {
            if let Some(cwd) = session.pane.cwd_path() {
                if let Some(dir) = get_transcript_dir(&cwd) {
                    if dir.exists() && !new_dirs.contains(&dir) {
                        new_dirs.push(dir);
                    }
                }
            }
        }

        // Get reference to watcher
        if let Some(watcher) = &mut self._watcher {
            // Unwatch old dirs
            for dir in &self.watched_dirs {
                if !new_dirs.contains(dir) {
                    let _ = watcher.unwatch(dir);
                }
            }

            // Watch new dirs
            for dir in &new_dirs {
                if !self.watched_dirs.contains(dir) {
                    let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
                }
            }
        }

        self.watched_dirs = new_dirs;

        Ok(())
    }

    /// Drain file change events and return true if any were received
    fn drain_file_changes(&self) -> bool {
        let mut had_changes = false;
        if let Some(rx) = &self.file_change_rx {
            while rx.try_recv().is_ok() {
                had_changes = true;
            }
        }
        had_changes
    }

    /// Extract current workspace from pane list (avoids redundant wezterm CLI call)
    fn extract_current_workspace(panes: &[crate::models::Pane]) -> Option<String> {
        let current_pane_id = std::env::var("WEZTERM_PANE").ok()?.parse::<u32>().ok()?;
        panes
            .iter()
            .find(|p| p.pane_id == current_pane_id)
            .map(|p| p.workspace.clone())
    }

    /// Apply duplicate CWD guard: clear last_prompt/last_output for sessions
    /// that share the same CWD without statusLine bridge mapping.
    fn apply_duplicate_cwd_guard(&mut self) {
        // Count sessions per cwd (only those without mapping AND without warning)
        let mut cwd_counts: HashMap<String, usize> = HashMap::new();
        for session in &self.sessions {
            if session.session_id.is_none() && session.warning.is_none() {
                if let Some(cwd) = session.pane.cwd_path() {
                    *cwd_counts.entry(cwd).or_insert(0) += 1;
                }
            }
        }

        // Clear last_prompt/last_output for sessions with duplicate cwd (without mapping)
        for session in &mut self.sessions {
            if session.session_id.is_some() || session.warning.is_some() {
                continue;
            }
            if let Some(cwd) = session.pane.cwd_path() {
                if cwd_counts.get(&cwd).copied().unwrap_or(0) > 1 {
                    session.last_prompt = None;
                    session.last_output =
                        Some("Run `wzcc install-bridge` for multi-session support".to_string());
                }
            }
        }
    }

    /// Lightweight refresh: only re-read transcript data for known sessions.
    /// Does NOT call wezterm CLI, ps, or git. Only re-reads transcript files.
    fn refresh_transcripts(&mut self) {
        for session in &mut self.sessions {
            let info = ClaudeSession::detect_session_info(&session.pane);
            session.status = info.status;
            session.last_prompt = info.last_prompt;
            session.last_output = info.last_output;
            session.updated_at = info.updated_at;
            session.warning = info.warning;
            session.session_id = info.session_id;
            session.transcript_path = info.transcript_path;
        }
        self.apply_duplicate_cwd_guard();
        self.dirty = true;
    }

    /// Check if enough time has passed for a debounced transcript refresh.
    /// Uses trailing-edge debounce: if not enough time passed, sets pending flag.
    fn should_refresh_transcripts(&mut self) -> bool {
        let debounce = Duration::from_millis(500);
        if self.last_transcript_refresh.elapsed() >= debounce {
            self.pending_transcript_refresh = false;
            self.last_transcript_refresh = Instant::now();
            true
        } else {
            self.pending_transcript_refresh = true;
            false
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

        // Get all panes (single call, also used to extract workspace)
        let panes = self.pane_ds.list_panes()?;

        // Extract workspace from pane list (avoids redundant wezterm CLI call)
        self.current_workspace = Self::extract_current_workspace(&panes)
            .unwrap_or_else(|| self.current_workspace.clone());

        // Build process tree once (optimization)
        let process_tree = self.process_ds.build_tree()?;

        self.sessions = panes
            .into_iter()
            .filter_map(|pane| {
                // Try to detect Claude Code (reusing process tree)
                let reason = self
                    .detector
                    .detect_by_tty_with_tree(&pane, &process_tree)
                    .ok()??;

                // Get session info (uses statusLine bridge if available, falls back to CWD-based)
                let session_info = ClaudeSession::detect_session_info(&pane);

                // Keep only detected sessions (git_branch filled below)
                Some(ClaudeSession {
                    pane,
                    detected: true,
                    reason,
                    status: session_info.status,
                    git_branch: None,
                    last_prompt: session_info.last_prompt,
                    last_output: session_info.last_output,
                    session_id: session_info.session_id,
                    transcript_path: session_info.transcript_path,
                    updated_at: session_info.updated_at,
                    warning: session_info.warning,
                })
            })
            .collect();

        // Fill in git branches with caching (separate loop to avoid borrow issues)
        for session in &mut self.sessions {
            if let Some(cwd) = session.pane.cwd_path() {
                session.git_branch = self.git_branch_cache.get(&cwd);
            }
        }

        // Apply duplicate CWD guard
        self.apply_duplicate_cwd_guard();

        // Sort by workspace → cwd → pane_id
        // Current workspace comes first
        let current_ws = self.current_workspace.clone();
        self.sessions.sort_by(|a, b| {
            // Current workspace should come first
            let ws_a_is_current = a.pane.workspace == current_ws;
            let ws_b_is_current = b.pane.workspace == current_ws;
            match (ws_a_is_current, ws_b_is_current) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    // Same priority, sort by workspace name, then cwd, then pane_id
                    let ws_a = &a.pane.workspace;
                    let ws_b = &b.pane.workspace;
                    let cwd_a = a.pane.cwd_path().unwrap_or_default();
                    let cwd_b = b.pane.cwd_path().unwrap_or_default();
                    ws_a.cmp(ws_b)
                        .then(cwd_a.cmp(&cwd_b))
                        .then(a.pane.pane_id.cmp(&b.pane.pane_id))
                }
            }
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
                let target_workspace = &session.pane.workspace;
                let switching_workspace = target_workspace != &self.current_workspace;

                // Switch workspace if needed
                if switching_workspace {
                    switch_workspace(target_workspace)?;
                }

                // Activate pane
                WeztermCli::activate_pane(pane_id)?;

                // Refresh session list after workspace switch to update ordering
                if switching_workspace {
                    // Small delay to allow WezTerm to complete workspace switch
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    self.refresh()?;
                }
            }
        }

        Ok(())
    }

    /// Calculate session index from list display row
    /// Returns the session corresponding to the clicked row, considering group headers
    fn row_to_session_index(&self, row: usize) -> Option<usize> {
        // Map row number to session index
        let mut current_row = 0;
        let mut current_ws: Option<String> = None;
        let mut current_cwd: Option<String> = None;

        for (session_idx, session) in self.sessions.iter().enumerate() {
            let ws = &session.pane.workspace;
            let cwd = session.pane.cwd_path().unwrap_or_default();

            // Add header row for new workspace
            if current_ws.as_ref() != Some(ws) {
                current_ws = Some(ws.clone());
                current_cwd = None; // Reset cwd for new workspace
                                    // Workspace header row
                if current_row == row {
                    // Ignore header click (not a session)
                    return None;
                }
                current_row += 1;
            }

            // Add header row for new CWD
            if current_cwd.as_ref() != Some(&cwd) {
                current_cwd = Some(cwd.clone());
                // CWD header row
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

    /// Enter input mode
    fn enter_input_mode(&mut self) {
        if self.list_state.selected().is_some() && !self.sessions.is_empty() {
            self.input_mode = true;
            self.input_buffer.clear();
            self.dirty = true;
            self.needs_full_redraw = true;
        }
    }

    /// Exit input mode
    fn exit_input_mode(&mut self) {
        self.input_mode = false;
        self.input_buffer.clear();
        self.dirty = true;
        self.needs_full_redraw = true;
    }

    /// Send prompt to the selected session
    fn send_prompt(&mut self) -> Result<()> {
        let text = self.input_buffer.as_str().trim().to_string();
        if text.is_empty() {
            self.toast = Some(Toast::error("Empty prompt".to_string()));
            self.dirty = true;
            return Ok(());
        }

        if let Some(i) = self.list_state.selected() {
            if let Some(session) = self.sessions.get(i) {
                let pane_id = session.pane.pane_id;
                let target_workspace = session.pane.workspace.clone();
                let switching_workspace = target_workspace != self.current_workspace;

                // Send text to pane
                match WeztermCli::send_text(pane_id, &text) {
                    Ok(()) => {
                        // Switch workspace if needed
                        if switching_workspace {
                            let _ = switch_workspace(&target_workspace);
                        }

                        // Activate pane
                        let _ = WeztermCli::activate_pane(pane_id);

                        self.toast = Some(Toast::success(format!("Sent to Pane {}", pane_id)));
                    }
                    Err(e) => {
                        self.toast = Some(Toast::error(format!("Failed: {}", e)));
                    }
                }
            }
        }

        self.exit_input_mode();
        Ok(())
    }

    /// Run TUI
    pub fn run(&mut self) -> Result<()> {
        // Clean up stale session mappings for TTYs that no longer exist
        // This prevents stale data from affecting new sessions on the same TTY
        self.cleanup_inactive_session_mappings();

        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Setup file watcher
        self.setup_watcher()?;

        // Initial refresh
        self.refresh()?;

        // Start watching transcript directories
        self.update_watched_dirs()?;

        // Event handler - shorter poll interval (100ms) since we're event-driven now
        // This is just for keyboard/mouse events, not for status updates
        let event_handler = EventHandler::new(100);

        // Track last full refresh time (for new session detection)
        let mut last_full_refresh = std::time::Instant::now();
        let full_refresh_interval = std::time::Duration::from_secs(5);

        // Main loop
        let result = loop {
            // Check for file changes from notify (lightweight transcript-only refresh)
            if self.drain_file_changes() && self.should_refresh_transcripts() {
                self.refresh_transcripts();

                // Check for actual changes in output
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

            // Clear expired toast
            if let Some(ref toast) = self.toast {
                if toast.is_expired() {
                    self.toast = None;
                    self.dirty = true;
                }
            }

            // Event processing
            match event_handler.next()? {
                Event::Key(key) if self.input_mode => {
                    // Input mode key handling
                    match key.code {
                        KeyCode::Esc => {
                            self.exit_input_mode();
                        }
                        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Ctrl+O -> newline
                            self.dirty |= self.input_buffer.insert_char('\n');
                        }
                        KeyCode::Enter => {
                            // Enter -> submit
                            self.send_prompt()?;
                        }
                        KeyCode::Backspace => {
                            self.dirty |= self.input_buffer.backspace();
                        }
                        KeyCode::Left => {
                            self.dirty |= self.input_buffer.cursor_left();
                        }
                        KeyCode::Right => {
                            self.dirty |= self.input_buffer.cursor_right();
                        }
                        KeyCode::Up => {
                            self.dirty |= self.input_buffer.cursor_up();
                        }
                        KeyCode::Down => {
                            self.dirty |= self.input_buffer.cursor_down();
                        }
                        KeyCode::Home => {
                            self.dirty |= self.input_buffer.cursor_home();
                        }
                        KeyCode::End => {
                            self.dirty |= self.input_buffer.cursor_end();
                        }
                        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.dirty |= self.input_buffer.cursor_home();
                        }
                        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.dirty |= self.input_buffer.cursor_end();
                        }
                        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if !self.input_buffer.is_empty() {
                                self.input_buffer.clear();
                                self.dirty = true;
                            }
                        }
                        KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.dirty |= self.input_buffer.cursor_left();
                        }
                        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.dirty |= self.input_buffer.cursor_down();
                        }
                        KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.dirty |= self.input_buffer.cursor_up();
                        }
                        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.dirty |= self.input_buffer.cursor_right();
                        }
                        KeyCode::Char(c) => {
                            self.dirty |= self.input_buffer.insert_char(c);
                        }
                        _ => {}
                    }
                }
                Event::Key(key) => {
                    // Normal mode key handling
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
                    } else if key.code == KeyCode::Char('h') {
                        // Expand details panel (move divider left)
                        if self.details_width_percent < 80 {
                            self.details_width_percent += 5;
                            self.dirty = true;
                            self.needs_full_redraw = true;
                        }
                    } else if key.code == KeyCode::Char('l') {
                        // Shrink details panel (move divider right)
                        if self.details_width_percent > 20 {
                            self.details_width_percent -= 5;
                            self.dirty = true;
                            self.needs_full_redraw = true;
                        }
                    } else if key.code == KeyCode::Char('i') {
                        // Enter input mode
                        self.enter_input_mode();
                    } else if is_refresh_key(&key) {
                        // Show refreshing indicator then update
                        self.refreshing = true;
                        self.dirty = true;
                        terminal.draw(|f| self.render(f))?;
                        self.git_branch_cache.clear();
                        self.refresh()?;
                        self.refreshing = false;
                    } else if let KeyCode::Char(c) = key.code {
                        // Quick select with number keys [1-9]
                        if let Some(digit) = c.to_digit(10) {
                            if (1..=9).contains(&digit) {
                                let index = (digit - 1) as usize;
                                if index < self.sessions.len() {
                                    self.list_state.select(Some(index));
                                    self.dirty = true;
                                    // Also jump to the session
                                    let _ = self.jump_to_selected();
                                }
                            }
                        }
                    }
                }
                Event::Mouse(mouse) if self.input_mode => {
                    // Ignore mouse in input mode
                    let _ = mouse;
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
                    // Advance animation frame for Processing indicator
                    self.animation_frame = (self.animation_frame + 1) % 4;

                    // Trigger redraw if any session is Processing (for animation)
                    let has_processing = self
                        .sessions
                        .iter()
                        .any(|s| matches!(s.status, crate::transcript::SessionStatus::Processing));
                    if has_processing {
                        self.dirty = true;
                    }

                    // Flush pending transcript refresh (trailing-edge debounce)
                    if self.pending_transcript_refresh
                        && self.last_transcript_refresh.elapsed() >= Duration::from_millis(500)
                    {
                        self.refresh_transcripts();
                        self.pending_transcript_refresh = false;
                        self.last_transcript_refresh = Instant::now();

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

                    // Periodic full refresh for new session detection (every 5 seconds)
                    if last_full_refresh.elapsed() >= full_refresh_interval {
                        self.refresh()?;
                        self.update_watched_dirs()?;
                        last_full_refresh = std::time::Instant::now();

                        // Check for actual changes in output
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

        // Vertical layout: main content + footer
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(size);

        let main_area = vertical_chunks[0];
        let footer_area = vertical_chunks[1];

        // 2-column layout (left: list, right: details - resizable with h/l)
        let list_percent = 100 - self.details_width_percent;
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(list_percent),
                Constraint::Percentage(self.details_width_percent),
            ])
            .split(main_area);

        // Render list (update list_area)
        self.list_area = render_list(
            f,
            chunks[0],
            &self.sessions,
            &mut self.list_state,
            self.refreshing,
            self.animation_frame,
            &self.current_workspace,
        );

        // Render details
        render_details(
            f,
            chunks[1],
            &self.sessions,
            self.list_state.selected(),
            self.input_mode,
            self.input_buffer.as_str(),
            self.input_buffer.cursor(),
        );

        // Render footer with keybindings help
        render_footer(f, footer_area, self.input_mode, self.toast.as_ref());
    }
}
