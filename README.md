<div align="center">

# wzcc

## WezTerm Claude Code Session Manager

A TUI tool to list and navigate Claude Code sessions running in WezTerm tabs/panes. Quickly jump between multiple Claude Code instances with intelligent session detection.

<div align="center">
  <a href="https://github.com/K-dash/wzcc/graphs/commit-activity"><img alt="GitHub commit activity" src="https://img.shields.io/github/commit-activity/m/K-dash/wzcc"/></a>
  <a href="https://github.com/K-dash/wzcc/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/LICENSE-MIT-green"/></a>
  <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/rust-1.70+-orange.svg"/></a>
</div>

<p>
  <a href="#quick-start">Quick Start</a>
  ◆ <a href="#features">Features</a>
  ◆ <a href="#installation">Installation</a>
  ◆ <a href="#architecture">Architecture</a>
</p>

<img width="1788" height="1072" alt="image" src="https://github.com/user-attachments/assets/dc57ceb8-75b9-4135-958e-4cb21953a3a1" />

</div>

## What is wzcc?

wzcc simplifies management of multiple Claude Code sessions in WezTerm. Instead of manually navigating between tabs and panes, wzcc provides an interactive TUI that displays all active Claude Code sessions with their status and context.

**Key capabilities:**
- Auto-detects Claude Code sessions using TTY matching + process tree analysis
- Shows session status: Processing, Idle, Ready, or Waiting for user approval
- Displays last user prompt and assistant response for quick context
- Extracts and displays git branch from each session's working directory
- One-keystroke navigation between sessions

## Features

### Session Detection
- **TTY Matching**: Matches WezTerm pane TTY with running process TTY
- **Wrapper Support**: Detects Claude Code even when launched through wrapper scripts
- **Process Tree Analysis**: Uses ancestor process checking to identify wrapped sessions
- **Default Allowlist**: Detects processes named `claude` or `anthropic` (customization requires code modification)

### Session Information
- **Status Detection**: Reads Claude Code transcript files to determine session status
  - `Ready`: Fresh session, no transcript entries yet or only internal entries
  - `Processing`: Last entry is progress, tool_result, user input, or recent tool_use (<10s)
  - `Idle`: Last entry is assistant response, end_turn, turn_duration, or stop_hook_summary
  - `Waiting`: Tool invocation pending user approval (>10s timeout by default)
  - `Unknown`: Status cannot be determined
- **Context Display**: Shows last user prompt and assistant response
- **Git Integration**: Extracts git branch name from session working directory
- **Pane Details**: Displays pane ID, working directory, TTY, status, and git branch

### User Interface
- **Multi-Workspace Support**: Shows sessions from all workspaces, grouped with visual hierarchy (🏠 current, 📍 others)
- **Cross-Workspace Navigation**: Jump to sessions in different workspaces with automatic workspace switching
- **Real-time Updates**: Uses `notify` crate to watch transcript files for changes - status updates instantly without polling
- **Efficient Rendering**: Event-driven, only redraws when state changes
- **Quick Select**: Press `1-9` to instantly jump to a session (numbers shown in list)
- **Relative Time Display**: Shows elapsed time since last activity (e.g., `5s`, `2m`, `1h`)
- **Keybindings Help**: Footer shows available keybindings at a glance
- **Keybindings**: vim-style (`j`/`k`) and arrow keys for navigation
- **Double-click Support**: Click list items to jump
- **Live Refresh**: `r` key refreshes session list

## Quick Start

### Prerequisites

- **WezTerm v20240203-110809 or later**
  - Must be run **inside WezTerm** (relies on `WEZTERM_PANE` environment variable)
  - Does not work in external terminals or SSH sessions
- **macOS 14+** (Linux support in progress)
- **Rust 1.70+** (to build from source)

### Installation

```bash
# Clone the repository
git clone https://github.com/K-dash/wzcc.git
cd wzcc

# Build release binary
cargo build --release

# Install to ~/.cargo/bin
cargo install --path .
```

### Running

```bash
# Start the TUI (lists all Claude Code sessions in current workspace)
wzcc

# Or explicitly specify TUI mode
wzcc tui

# Start background daemon (experimental - updates tab titles with session status)
wzcc daemon
```

**Note**: Daemon mode is experimental. It monitors sessions in the current workspace and updates WezTerm tab titles with status. Polling interval is 3 seconds. Currently only works in the current workspace.

### Using wzcc

**Keybindings:**

| Key | Action |
|-----|--------|
| `j` / `↓` | Move to next session |
| `k` / `↑` | Move to previous session |
| `1-9` | Quick select & focus session by number |
| `g` + `g` | Jump to first session |
| `G` | Jump to last session |
| `Enter` / Double-click | Switch to selected session (TUI continues) |
| `c` | Quit TUI |
| `q` / `Esc` | Quit TUI |
| `r` | Refresh session list |

## Installation

### From Source

```bash
# Clone repository
git clone https://github.com/K-dash/wzcc.git
cd wzcc

# Build and install
cargo install --path .
```

### Verify Installation

```bash
wzcc --version
```

## Architecture

### Data Flow

```mermaid
flowchart TD
    A[User Input] -->|keyboard/mouse events| B

    B --> C

    C --> D

    D -->|Enter key| E[Action Execution]
    D -->|j/k/↑/↓| D

    E -->|activate pane| F[WezTerm Pane]
    E -->|TUI continues| D

    subgraph B [Session Detection]
        direction TB
        B1[Fetch panes from WezTerm CLI]
        B2[Build process tree from ps]
        B3[Match pane TTY → process TTY]
        B4[Check allowlist & wrapper detection]
    end

    subgraph C [Session Info Enrichment]
        direction TB
        C1[Extract git branch from CWD]
        C2[Parse transcript files]
        C3[Determine session status]
        C4[Extract last prompt & response]
    end

    subgraph D [TUI Rendering]
        direction TB
        D1[Render session list with status]
        D2[Display selected session details]
        D3[Reactive rendering on state change]
    end
```

### Session Status Detection

wzcc reads Claude Code transcript files located in `~/.claude/projects/{encoded-cwd}/{session_id}.jsonl` (where `encoded-cwd` replaces `/`, `.`, and `_` with `-`) and examines the transcript structure to determine session status:

| Status | Condition |
|--------|-----------|
| `Processing` | Last entry is progress event, tool_result from user, user input (Claude responding), or recent tool_use invocation (<10 seconds) |
| `Idle` | Last entry is assistant response, end_turn marker, turn_duration, or stop_hook_summary |
| `Waiting` | Tool use pending user approval (>10 seconds elapsed) |
| `Ready` | Fresh session with no meaningful entries yet |
| `Unknown` | Transcript parsing failed or status cannot be determined |

## Limitations

### Cross-Workspace Navigation

wzcc displays sessions from **all workspaces** and can switch between them. However, this requires a one-time setup since WezTerm CLI doesn't provide a native workspace switch command.

**Solution: Install the workspace switcher**

```bash
wzcc install-workspace-switcher
```

This command injects a Lua snippet into your `wezterm.lua` that listens for OSC 1337 escape sequences and performs workspace switches.

**After installation**, restart WezTerm or reload config (`Ctrl+Shift+R`) for changes to take effect.

**To uninstall:**

```bash
wzcc uninstall-workspace-switcher
```

**Without the switcher:**
- Sessions from all workspaces are still displayed
- Jumping to a session in a different workspace will activate the pane but not switch the workspace view

### Multiple Sessions with Same Working Directory

When multiple Claude Code sessions share the same working directory, wzcc needs additional setup to accurately display each session's status and output. Without the statusLine bridge, session identification relies on transcript paths using encoded working directory names, which cannot distinguish between multiple sessions in the same directory.

**Solution: Install the statusLine bridge**

```bash
wzcc install-bridge
```

This command:
1. Creates a bridge script at `~/.claude/wzcc_statusline_bridge.sh`
2. Configures Claude Code's `statusLine.command` to use the bridge
3. Preserves any existing statusLine configuration by chaining calls

The bridge leverages Claude Code's statusLine feature (which updates every 300ms) to write session information keyed by TTY. This allows wzcc to accurately identify and display each session even when multiple sessions share the same CWD.

**After installation**, restart your Claude Code sessions for the changes to take effect.

**To uninstall:**

```bash
wzcc uninstall-bridge
```

**Without the bridge:**
- Session status detection still works but may show wrong data for multi-CWD sessions
- A message prompts you to run `wzcc install-bridge`
- Individual pane IDs remain accurate

### Platform Support

| Platform | Status |
|----------|--------|
| macOS 14+ | ✅ Full support |
| Linux | 🚧 Experimental |
| Windows | ❌ Not supported |
| Remote Mux | ❌ Not supported |

## Development

### Build

```bash
# Debug build
cargo build

# Release build (recommended)
cargo build --release
make release
```

### Run

```bash
# Run TUI
cargo run

# Run with debug output
RUST_LOG=debug cargo run
```

### Testing & Code Quality

```bash
# Run all tests
cargo test
make test

# Run specific test
cargo test test_detect_wrapper_detected

# Format code
cargo fmt
make fmt

# Lint with clippy
cargo clippy -- -D warnings
make lint

# Run full CI checks (format check, lint, test)
make ci
```

## License

This project is licensed under the **MIT License** - see the [LICENSE](LICENSE) file for details.
