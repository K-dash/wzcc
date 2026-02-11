# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

---

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
- [ ] Issue verification criteria satisfied (if linked to a beads issue)?

## Instructions for AI Agents

- Before committing, ALWAYS re-read this Workflow section
- When user says "commit", first check current branch and create feature branch if on main
- When user-facing behavior changes, proactively update README.md before committing
- **All code comments, commit messages, PR titles, PR descriptions, and review comments MUST be written in English**

### Plan-First Rule

For changes touching **3 or more files** or introducing **new architectural patterns**:

1. **Enter plan mode first** — use `EnterPlanMode` to explore the codebase and design the approach before writing any code.
2. **Get the plan approved** — the user must approve before execution begins. The plan is the contract.
3. **Include a verification strategy** — every plan must answer: "How will we verify this works?" (tests, manual checks, CI gates, etc.)
4. **Stop if scope drifts** — if the implementation diverges from the approved plan, stop and re-plan rather than improvising.

For small, well-scoped changes (single-file fixes, typo corrections, simple bug fixes), skip planning and execute directly.

## Code Style

- Rust 2021 edition
- Use `cargo fmt` for formatting
- All clippy warnings treated as errors (`-D warnings`)

## Testing

- Run single test: `cargo test test_name`
- Run all tests: `cargo test` or `make test`
- Tests located alongside source in same module or in tests/ directory

## Issue Tracking (Beads)

This project uses `bd` (beads) for issue tracking. See the "Landing the Plane" section above for session completion workflow.

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

### Verification Spec

Every issue description MUST include a **## Verification** section before work begins. This defines how we prove the change works and prevents "it compiles, ship it" mentality.

When creating an issue with `bd create`, always include verification criteria in the description:

```
## Verification
- [ ] `cargo test` passes (unit tests cover the new logic)
- [ ] Manual test: <specific scenario to try>
- [ ] No clippy warnings introduced
```

**What to include** (pick the relevant ones):
- **Unit tests**: Which functions need new or updated tests?
- **Manual test steps**: Exact steps to reproduce / verify the fix (e.g., "open wzcc, close a pane, confirm it disappears within 2s")
- **Edge cases**: Specific scenarios that must not break (e.g., "zero sessions", "100+ sessions", "pane closed mid-render")
- **Regression check**: What existing behavior must remain unchanged?

When closing an issue, verify ALL items in the Verification section are satisfied. If any are skipped, document why.

## Project Structure

- `src/ui/` - TUI components and event handling
- `src/detector/` - Claude Code session detection logic
- `src/transcript/` - Transcript file parsing
- `src/datasource/` - WezTerm and system data retrieval

---

## Known Mistakes & Lessons Learned

Record AI-generated mistakes and the rules that prevent them from recurring. Update this section after every code review where the AI got something wrong. This knowledge compounds over time.

<!-- Add entries in reverse-chronological order (newest first) -->
<!-- Format: ### YYYY-MM-DD: Short description -->
<!-- - **What happened**: ... -->
<!-- - **Root cause**: ... -->
<!-- - **Rule**: The constraint to prevent recurrence -->

### 2025-06: Stale WezTerm pane data after close
- **What happened**: AI assumed `wezterm cli list` output was always fresh; closed panes sometimes lingered in output for a few seconds.
- **Root cause**: WezTerm CLI output is eventually consistent, not immediately consistent after pane close.
- **Rule**: Always treat pane listings as potentially stale. Filter or retry when pane operations fail.

### 2025-06: Overly broad `unwrap()` usage
- **What happened**: AI used `.unwrap()` on fallible operations in TUI rendering paths, causing panics on edge cases.
- **Root cause**: TUI apps must not panic — a crash kills the terminal state.
- **Rule**: Never use `.unwrap()` in rendering or event-handling code. Use `.unwrap_or_default()`, `if let`, or propagate errors. `.unwrap()` is acceptable only in tests and proven-infallible cases (e.g., static regex compilation).

## Architecture Decisions

Key design choices and their rationale. Helps AI agents understand *why* things are the way they are, not just *what* they are.

### TTY-based session detection (not process-name matching)
- **Context**: Need to map WezTerm panes to Claude Code sessions.
- **Decision**: Match by TTY device (`/dev/ttysNNN`) from `ps` output, cross-referenced with WezTerm pane TTY assignments.
- **Alternatives considered**: (1) Process name matching — fragile with wrappers/aliases. (2) WezTerm `foreground_process_name` field — unreliable and sometimes empty.
- **Trade-off**: ~90% accuracy, but robust across wrapper scripts and shell configurations.

### Transcript file parsing for session status
- **Context**: Need to show what each Claude Code session is doing (idle, thinking, tool use, etc.).
- **Decision**: Parse Claude Code's JSONL transcript files directly from `~/.claude/projects/`.
- **Alternatives considered**: (1) Screen-scraping pane content — brittle and lossy. (2) Claude Code API — no public API exists.
- **Trade-off**: Tight coupling to transcript format (may break on Claude Code updates), but gives rich session state.

