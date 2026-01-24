use anyhow::Result;
use clap::{Parser, Subcommand};
use wzcc::ui::App;

#[derive(Parser)]
#[command(name = "wzcc")]
#[command(about = "WezTerm Claude Code - TUI for managing Claude Code sessions")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start TUI mode (default)
    Tui,
    /// Start daemon mode (background monitoring)
    Daemon,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => {
            let mut app = App::new();
            app.run()?;
        }
        Some(Commands::Daemon) => {
            run_daemon()?;
        }
    }

    Ok(())
}

fn run_daemon() -> Result<()> {
    println!("Starting wzcc daemon...");

    // TODO: 実装
    // 1. 現在の workspace の Claude Code セッションを検出
    // 2. トランスクリプトファイルを監視
    // 3. ステータス変化時にタブ名を変更

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        wzcc::daemon::run().await
    })
}
