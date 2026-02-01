use anyhow::Result;
use clap::{Parser, Subcommand};
use wzcc::cli::{
    install_bridge, install_workspace_switcher, uninstall_bridge, uninstall_workspace_switcher,
};
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
    /// Install statusLine bridge for multi-session support
    InstallBridge,
    /// Uninstall statusLine bridge
    UninstallBridge,
    /// Install workspace switcher for cross-workspace navigation
    InstallWorkspaceSwitcher,
    /// Uninstall workspace switcher
    UninstallWorkspaceSwitcher,
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
        Some(Commands::InstallBridge) => {
            install_bridge()?;
        }
        Some(Commands::UninstallBridge) => {
            uninstall_bridge()?;
        }
        Some(Commands::InstallWorkspaceSwitcher) => {
            install_workspace_switcher()?;
        }
        Some(Commands::UninstallWorkspaceSwitcher) => {
            uninstall_workspace_switcher()?;
        }
    }

    Ok(())
}

fn run_daemon() -> Result<()> {
    println!("Starting wzcc daemon...");

    // TODO: Implementation
    // 1. Detect Claude Code sessions in current workspace
    // 2. Watch transcript files
    // 3. Change tab name on status change

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { wzcc::daemon::run().await })
}
