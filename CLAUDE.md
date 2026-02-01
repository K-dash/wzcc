# wzcc - WezTerm Claude Code Session Manager

See @README.md for project overview.
See @docs/internal/project-structure.md for architecture details.

## Build & Quality

```bash
# REQUIRED: Run before completing any work
make all          # format + lint + test

# Individual commands
make fmt          # cargo fmt
make lint         # cargo clippy -- -D warnings
make test         # cargo test
```

## Workflow

- Always create a feature branch before making changes
- Commit changes using conventional commits (feat:, fix:, docs:, etc.)
- Create a PR to merge into main
- IMPORTANT: Run `make all` before considering work complete

## Code Style

- Rust 2021 edition
- Use `cargo fmt` for formatting
- All clippy warnings treated as errors (`-D warnings`)

## Testing

- Run single test: `cargo test test_name`
- Run all tests: `cargo test` or `make test`
- Tests located alongside source in same module or in tests/ directory

## Project Structure

- `src/ui/` - TUI components and event handling
- `src/detector/` - Claude Code session detection logic
- `src/transcript/` - Transcript file parsing
- `src/datasource/` - WezTerm and system data retrieval
