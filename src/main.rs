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
    /// Install all components (bridge + workspace-switcher)
    Install,
    /// Uninstall all components (bridge + workspace-switcher)
    Uninstall,
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
        Some(Commands::Install) => {
            println!("Installing all wzcc components...\n");
            install_bridge()?;
            println!();
            install_workspace_switcher()?;
            println!("\n✓ All components installed successfully!");
        }
        Some(Commands::Uninstall) => {
            println!("Uninstalling all wzcc components...\n");
            uninstall_bridge()?;
            println!();
            uninstall_workspace_switcher()?;
            println!("\n✓ All components uninstalled successfully!");
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
