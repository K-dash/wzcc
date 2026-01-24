//! Daemon watcher for monitoring Claude Code sessions.

use crate::cli::WeztermCli;
use crate::datasource::{PaneDataSource, SystemProcessDataSource, WeztermDataSource};
use crate::detector::ClaudeCodeDetector;
use crate::models::Pane;
use crate::transcript::{detect_session_status, get_latest_transcript, get_transcript_dir, SessionStatus};
use anyhow::Result;
use std::collections::HashMap;
use tokio::time::{interval, Duration};

/// Session info with cached status
struct SessionInfo {
    #[allow(dead_code)]
    pane: Pane,
    status: SessionStatus,
    original_title: String,
}

/// Run the daemon
pub async fn run() -> Result<()> {
    let pane_ds = WeztermDataSource::new();
    let process_ds = SystemProcessDataSource::new();
    let detector = ClaudeCodeDetector::new();

    let mut sessions: HashMap<u32, SessionInfo> = HashMap::new();

    // 3秒ごとにポーリング
    let mut ticker = interval(Duration::from_secs(3));

    println!("Daemon started. Monitoring Claude Code sessions...");
    println!("Press Ctrl+C to stop.");

    loop {
        ticker.tick().await;

        // 現在の workspace を取得
        let current_workspace = match pane_ds.get_current_workspace() {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("Failed to get current workspace: {}", e);
                continue;
            }
        };

        // Pane 一覧を取得
        let panes = match pane_ds.list_panes() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to list panes: {}", e);
                continue;
            }
        };

        // 現在のセッションを追跡
        let mut current_pane_ids: Vec<u32> = Vec::new();

        for pane in panes {
            // 現在の workspace のみ対象
            if pane.workspace != current_workspace {
                continue;
            }

            // Claude Code 検出
            let is_claude = detector
                .detect_by_tty(&pane, &process_ds)
                .ok()
                .flatten()
                .is_some();

            if !is_claude {
                // Claude Code じゃなくなった場合、タイトルを元に戻す
                if let Some(info) = sessions.remove(&pane.pane_id) {
                    let _ = WeztermCli::set_tab_title(pane.pane_id, &info.original_title);
                    println!(
                        "Pane {} is no longer Claude Code, restored title",
                        pane.pane_id
                    );
                }
                continue;
            }

            current_pane_ids.push(pane.pane_id);

            // セッション状態を取得
            let status = detect_status_for_pane(&pane);

            // 既存のセッションか確認
            if let Some(info) = sessions.get_mut(&pane.pane_id) {
                // 状態が変わった場合のみ更新
                if info.status != status {
                    let old_status = info.status.clone();
                    info.status = status.clone();

                    // タブタイトルを更新
                    let new_title = format_title(&info.original_title, &status);
                    if let Err(e) = WeztermCli::set_tab_title(pane.pane_id, &new_title) {
                        eprintln!("Failed to set tab title: {}", e);
                    } else {
                        println!(
                            "Pane {} status changed: {:?} -> {:?}",
                            pane.pane_id, old_status, status
                        );
                    }
                }
            } else {
                // 新しいセッション
                let original_title = pane.title.clone();
                let new_title = format_title(&original_title, &status);

                if let Err(e) = WeztermCli::set_tab_title(pane.pane_id, &new_title) {
                    eprintln!("Failed to set tab title: {}", e);
                } else {
                    println!(
                        "New Claude Code session detected: Pane {} ({:?})",
                        pane.pane_id, status
                    );
                }

                sessions.insert(
                    pane.pane_id,
                    SessionInfo {
                        pane,
                        status,
                        original_title,
                    },
                );
            }
        }

        // 消えたセッションを削除（タイトルを元に戻す）
        let gone_pane_ids: Vec<u32> = sessions
            .keys()
            .filter(|id| !current_pane_ids.contains(id))
            .copied()
            .collect();

        for pane_id in gone_pane_ids {
            if let Some(info) = sessions.remove(&pane_id) {
                // Pane が消えた場合はタイトル復元を試みない（エラーになる）
                println!("Pane {} closed", pane_id);
                let _ = info; // suppress unused warning
            }
        }
    }
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

/// タイトルにステータスアイコンを付加
fn format_title(original_title: &str, status: &SessionStatus) -> String {
    let icon = status.icon();
    format!("{} {}", icon, original_title)
}
