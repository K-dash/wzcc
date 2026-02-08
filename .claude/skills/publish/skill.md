---
name: publish
description: Bump version, tag, and push to trigger the automated release pipeline. Triggers on 'publish', 'release', 'bump version'. This skill handles the full version bump workflow and knows the CI/CD pipeline to avoid manual mistakes.
---

# Publish

## CI/CD Pipeline Overview

This project uses a two-stage GitHub Actions pipeline triggered by git tags:

1. **`release.yml`** - Triggered on `v*` tag push
   - Verifies tag version matches `Cargo.toml` version
   - Builds release binary for `aarch64-apple-darwin`
   - Creates a GitHub Release with the binary and checksum

2. **`publish.yml`** - Triggered when a GitHub Release is `published`
   - Verifies version consistency
   - Runs `cargo publish --locked` to publish to crates.io

**Flow**: `git push tag` → release.yml (build + GitHub Release) → publish.yml (crates.io)

## CRITICAL: Do NOT manually run `cargo publish`

The pipeline handles crates.io publishing automatically. Running `cargo publish` manually will cause the CI publish job to fail with a duplicate version error.

## Version Bump Workflow

When the user asks to publish or release a new version:

### Step 1: Pre-flight checks

```bash
git branch --show-current   # Must be on main
git status                  # Must be clean
git fetch --tags --quiet
```

- Confirm the working directory is clean and on `main`.
- Show the latest existing tag and current `Cargo.toml` version.

### Step 2: Ask for the new version

Use AskUserQuestion to ask which version to bump to. Show the current version and suggest options (patch, minor, major).

### Step 3: Update version

1. Edit `Cargo.toml` to set the new version.
2. Run `cargo check --quiet` to update `Cargo.lock`.
3. Show `git diff` for the user to review.

### Step 4: Commit, tag, and push

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to <NEW_VERSION>"
git tag v<NEW_VERSION>
git push origin main
git push origin v<NEW_VERSION>
```

### Step 5: Verify

After pushing, inform the user that the CI pipeline will:
1. Build the release binary
2. Create a GitHub Release
3. Publish to crates.io

Provide a link to check the Actions status:
`https://github.com/K-dash/wzcc/actions`
