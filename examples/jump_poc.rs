use anyhow::Result;
use wzcc::cli::WeztermCli;
use wzcc::datasource::{PaneDataSource, WeztermDataSource};

fn main() -> Result<()> {
    println!("=== wzcc Jump 機能 PoC ===\n");

    // ペイン一覧を取得
    let ds = WeztermDataSource::new();
    let panes = ds.list_panes()?;

    println!("Found {} panes:\n", panes.len());

    // ペインを番号付きで表示
    for (idx, pane) in panes.iter().enumerate() {
        let active_marker = if pane.is_active { " (ACTIVE)" } else { "" };
        println!(
            "[{}] Pane {} - {}{}",
            idx,
            pane.pane_id,
            pane.title,
            active_marker
        );
    }

    // 非 ACTIVE なペインを探す (テスト用)
    let target_pane = panes.iter().find(|p| !p.is_active);

    if let Some(pane) = target_pane {
        println!("\nTest: Jumping to pane {} ({})...", pane.pane_id, pane.title);

        // まず tab をアクティベート
        WeztermCli::activate_tab(pane.tab_id)?;

        // 次に pane をアクティベート
        WeztermCli::activate_pane(pane.pane_id)?;

        println!("✓ Jump succeeded!");
        println!("\n実際にペイン {} にジャンプしたか確認してください", pane.pane_id);
    } else {
        println!("\nAll panes are active (or only one pane exists)");
    }

    Ok(())
}
