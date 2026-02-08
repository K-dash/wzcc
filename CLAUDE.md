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

## Git Workflow (MUST FOLLOW)

⚠️ **NEVER commit directly to main. Always use feature branches.**

1. **BEFORE any code changes**: Create a feature branch
   ```bash
   git checkout -b feat/your-feature-name
   ```
2. **After changes**: Run quality checks
   ```bash
   make all  # format + lint + test
   ```
3. **Update documentation**: If user-facing behavior changes, update README.md
4. **Commit**: Use conventional commits (feat:, fix:, docs:, etc.)
5. **Push and create PR**: Never merge directly to main
   ```bash
   git push -u origin <branch-name>
   gh pr create
   ```

### Pre-Commit Checklist

Before committing, verify:
- [ ] On a feature branch (not main)?
- [ ] `make all` passes?
- [ ] README.md updated if needed?
- [ ] PR will be created?

## Instructions for Claude

- Before committing, ALWAYS re-read this Workflow section
- When user says "commit", first check current branch and create feature branch if on main
- When user-facing behavior changes, proactively update README.md before committing
- **All code comments, commit messages, PR titles, PR descriptions, and review comments MUST be written in English**

## Code Style

- Rust 2021 edition
- Use `cargo fmt` for formatting
- All clippy warnings treated as errors (`-D warnings`)

## Testing

- Run single test: `cargo test test_name`
- Run all tests: `cargo test` or `make test`
- Tests located alongside source in same module or in tests/ directory

## Issue Tracking (Beads)

This project uses `bd` (beads) for issue tracking. See @AGENTS.md for session completion workflow.

```bash
bd ready                          # Find available work (unblocked tasks)
bd show <id>                      # View issue details
bd create "Title" -p 1            # Create a new task
bd update <id> --claim            # Claim a task (assignee + in_progress)
bd close <id> --reason "..."      # Complete a task
bd comments add <id> "text"       # Add a comment (prefer over --notes)
bd sync                           # Sync with git
```

- Always run `bd ready` before starting work
- Never use `bd edit` (opens interactive editor)
- Include issue ID in commit messages: `feat: add feature (wzcc-xxxx)`
- Run `bd sync` at end of session

## Project Structure

- `src/ui/` - TUI components and event handling
- `src/detector/` - Claude Code session detection logic
- `src/transcript/` - Transcript file parsing
- `src/datasource/` - WezTerm and system data retrieval
