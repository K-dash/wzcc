use anyhow::Result;
use wzcc::datasource::{PaneDataSource, SystemProcessDataSource, WeztermDataSource};
use wzcc::detector::ClaudeCodeDetector;

fn main() -> Result<()> {
    println!("=== wzcc Claude Code Detection Test ===\n");

    // データソースを初期化
    let pane_ds = WeztermDataSource::new();
    let process_ds = SystemProcessDataSource::new();
    let detector = ClaudeCodeDetector::new();

    // 全ペインを取得
    let panes = pane_ds.list_panes()?;

    println!("Found {} panes\n", panes.len());

    let mut claude_count = 0;

    for pane in &panes {
        // Case 2: TTY マッチングで検出
        let reason = detector.detect_by_tty(pane, &process_ds)?;

        if let Some(reason) = reason {
            claude_count += 1;
            println!("✓ Claude Code detected:");
            println!("  Pane: {} (tab: {})", pane.pane_id, pane.tab_id);
            println!("  Title: {}", pane.title);
            println!("  Detection: {}", reason.display());
            if let Some(tty) = &pane.tty_name {
                println!("  TTY: {}", tty);
            }
            if let Some(cwd) = pane.cwd_path() {
                println!("  CWD: {}", cwd);
            }
            println!();
        }
    }

    println!("Total Claude Code sessions: {}", claude_count);

    Ok(())
}
