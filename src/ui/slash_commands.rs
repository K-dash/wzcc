use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// A single slash command entry for autocomplete.
#[derive(Debug, Clone)]
pub struct SlashCommand {
    /// Command name without leading slash (e.g. "compact")
    pub name: String,
    /// Short description shown in the autocomplete list
    pub description: String,
    /// Argument hint shown after the command name (e.g. "[instructions]")
    pub argument_hint: Option<String>,
    /// Source of this command for display grouping
    pub source: SlashCommandSource,
}

/// Where a slash command originates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SlashCommandSource {
    BuiltIn,
    PluginSkill,
    PluginCommand,
    UserSkill,
    UserCommand,
    ProjectSkill,
    ProjectCommand,
}

/// Built-in Claude Code commands (name, description).
const BUILTIN_COMMANDS: &[(&str, &str)] = &[
    ("clear", "Clear conversation history"),
    ("compact", "Compact conversation context"),
    ("config", "Open configuration"),
    ("context", "Visualize context usage"),
    ("copy", "Copy last response to clipboard"),
    ("cost", "Show token usage and cost"),
    ("debug", "Troubleshoot current session"),
    ("doctor", "Check system health"),
    ("exit", "Exit Claude Code"),
    ("export", "Export conversation to file"),
    ("help", "Show help"),
    ("init", "Initialize project with CLAUDE.md"),
    ("mcp", "Manage MCP server connections"),
    ("memory", "Edit CLAUDE.md memory files"),
    ("model", "Select or change AI model"),
    ("permissions", "View or update permissions"),
    ("plan", "Enter plan mode"),
    ("rename", "Rename current session"),
    ("resume", "Resume a conversation"),
    ("rewind", "Rewind conversation"),
    ("stats", "Show session stats"),
    ("status", "Show current status"),
    ("statusline", "Configure statusline"),
    ("tasks", "List and manage background tasks"),
    ("teleport", "Resume remote session"),
    ("theme", "Change color theme"),
    ("todos", "Show TODO items"),
    ("usage", "Show plan usage limits"),
    ("vim", "Toggle vim editor mode"),
];

/// YAML frontmatter fields we care about from SKILL.md files.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "argument-hint")]
    argument_hint: Option<String>,
    #[serde(rename = "user-invocable")]
    user_invocable: Option<bool>,
}

/// Extract the YAML frontmatter block from a SKILL.md file content.
/// Returns the text between the opening and closing `---` markers.
fn extract_frontmatter(content: &str) -> Option<&str> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..];
    let end_idx = after_first.find("\n---")?;
    Some(&after_first[..end_idx])
}

/// Parse a SKILL.md file into a SlashCommand.
/// Returns None if the file is invalid or user-invocable is false.
fn parse_skill_file(content: &str, dir_name: &str, source: SlashCommandSource) -> Option<SlashCommand> {
    let yaml_block = extract_frontmatter(content)?;
    let fm: SkillFrontmatter = serde_yaml::from_str(yaml_block).ok()?;

    // Skip skills explicitly marked as not user-invocable
    if fm.user_invocable == Some(false) {
        return None;
    }

    let name = fm.name.unwrap_or_else(|| dir_name.to_string());
    let description = fm.description.unwrap_or_default();

    Some(SlashCommand {
        name,
        description,
        argument_hint: fm.argument_hint,
        source,
    })
}

/// Parse a legacy command .md file into a SlashCommand.
/// The filename (without .md) becomes the command name.
/// Subdirectory paths use `:` as separator (e.g. "frontend:component").
fn parse_command_file(content: &str, relative_name: &str, source: SlashCommandSource) -> SlashCommand {
    // Use first non-empty line (stripped of heading markers) as description
    let description = content
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().trim_start_matches('#').trim().to_string())
        .unwrap_or_default();

    SlashCommand {
        name: relative_name.to_string(),
        description,
        argument_hint: None,
        source,
    }
}

/// Scan a skills directory (e.g. `~/.claude/skills/`) for SKILL.md files.
fn scan_skills_dir(dir: &Path, source: SlashCommandSource) -> Vec<SlashCommand> {
    let mut commands = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return commands,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }
        let dir_name = entry
            .file_name()
            .to_string_lossy()
            .to_string();
        let content = match std::fs::read_to_string(&skill_file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(cmd) = parse_skill_file(&content, &dir_name, source) {
            commands.push(cmd);
        }
    }
    commands
}

/// Scan a legacy commands directory (e.g. `~/.claude/commands/`) for .md files.
fn scan_commands_dir(dir: &Path, source: SlashCommandSource) -> Vec<SlashCommand> {
    let mut commands = Vec::new();
    scan_commands_dir_recursive(dir, dir, source, &mut commands);
    commands
}

/// Recursively scan for .md files, building `:` separated names for subdirectories.
fn scan_commands_dir_recursive(
    base: &Path,
    dir: &Path,
    source: SlashCommandSource,
    commands: &mut Vec<SlashCommand>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_commands_dir_recursive(base, &path, source, commands);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem.is_empty() {
                continue;
            }
            // Build relative name with `:` separators for subdirectories
            let relative_name = if let Ok(rel) = path.strip_prefix(base) {
                let parent = rel.parent().unwrap_or(Path::new(""));
                if parent.as_os_str().is_empty() {
                    stem.to_string()
                } else {
                    let prefix = parent
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join(":");
                    format!("{}:{}", prefix, stem)
                }
            } else {
                stem.to_string()
            };

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            commands.push(parse_command_file(&content, &relative_name, source));
        }
    }
}

/// JSON structure for installed_plugins.json
#[derive(Debug, Deserialize)]
struct InstalledPlugins {
    plugins: std::collections::HashMap<String, Vec<PluginEntry>>,
}

#[derive(Debug, Deserialize)]
struct PluginEntry {
    #[serde(rename = "installPath")]
    install_path: String,
}

/// Scan installed plugins for skills and commands.
/// Reads `~/.claude/plugins/installed_plugins.json` and scans each plugin's
/// `skills/` and `commands/` directories.
///
/// Plugin skill names are prefixed with `{plugin_name}:` (e.g. "Notion:find").
/// Plugin command names are prefixed with `{plugin_name}:` (e.g. "ralph-loop:help").
fn scan_installed_plugins() -> Vec<SlashCommand> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let plugins_file = home.join(".claude").join("plugins").join("installed_plugins.json");
    let content = match std::fs::read_to_string(&plugins_file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let installed: InstalledPlugins = match serde_json::from_str(&content) {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };

    let mut commands = Vec::new();

    for (qualified_name, entries) in &installed.plugins {
        // Extract plugin display name: "Notion@claude-plugins-official" -> "Notion"
        let plugin_name = qualified_name
            .split('@')
            .next()
            .unwrap_or(qualified_name);

        for entry in entries {
            let install_path = Path::new(&entry.install_path);

            // Scan skills/ directory
            let skills_dir = install_path.join("skills");
            if skills_dir.is_dir() {
                for cmd in scan_skills_dir(&skills_dir, SlashCommandSource::PluginSkill) {
                    // Prefix with plugin name: "pdf" -> "pdf" (already namespaced by dir)
                    // But for multi-skill plugins like Notion, the structure is
                    // skills/notion/find/ so we get "find" etc.
                    // Use plugin_name:skill_name format
                    let prefixed_name = format!("{}:{}", plugin_name, cmd.name);
                    commands.push(SlashCommand {
                        name: prefixed_name,
                        description: cmd.description,
                        argument_hint: cmd.argument_hint,
                        source: SlashCommandSource::PluginSkill,
                    });
                }

                // Some plugins nest skills in subdirectories (e.g. skills/notion/find/SKILL.md)
                // Scan one level deeper
                if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                    for sub_entry in entries.flatten() {
                        let sub_path = sub_entry.path();
                        if sub_path.is_dir() {
                            // Check if this is a skill dir (has SKILL.md) - already handled above
                            // Check if it contains subdirectories with SKILL.md (nested structure)
                            for cmd in scan_skills_dir(&sub_path, SlashCommandSource::PluginSkill) {
                                let prefixed_name = format!("{}:{}", plugin_name, cmd.name);
                                commands.push(SlashCommand {
                                    name: prefixed_name,
                                    description: cmd.description,
                                    argument_hint: cmd.argument_hint,
                                    source: SlashCommandSource::PluginSkill,
                                });
                            }
                        }
                    }
                }
            }

            // Scan commands/ directory
            let commands_dir = install_path.join("commands");
            if commands_dir.is_dir() {
                for cmd in scan_commands_dir(&commands_dir, SlashCommandSource::PluginCommand) {
                    let prefixed_name = format!("{}:{}", plugin_name, cmd.name);
                    commands.push(SlashCommand {
                        name: prefixed_name,
                        description: cmd.description,
                        argument_hint: cmd.argument_hint,
                        source: SlashCommandSource::PluginCommand,
                    });
                }
            }
        }
    }

    commands
}

/// Scan all slash command sources and return a merged, deduplicated list.
/// `session_cwd` is the selected session's working directory (for project-level skills).
pub fn scan_slash_commands(session_cwd: Option<&str>) -> Vec<SlashCommand> {
    let mut commands = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    // 1. Built-in commands
    for &(name, desc) in BUILTIN_COMMANDS {
        commands.push(SlashCommand {
            name: name.to_string(),
            description: desc.to_string(),
            argument_hint: None,
            source: SlashCommandSource::BuiltIn,
        });
        seen_names.insert(name.to_string());
    }

    // 2. Plugin skills and commands (~/.claude/plugins/)
    for cmd in scan_installed_plugins() {
        if seen_names.insert(cmd.name.clone()) {
            commands.push(cmd);
        }
    }

    // 3. User skills (~/.claude/skills/)
    if let Some(home) = dirs::home_dir() {
        let user_skills_dir = home.join(".claude").join("skills");
        for cmd in scan_skills_dir(&user_skills_dir, SlashCommandSource::UserSkill) {
            if seen_names.insert(cmd.name.clone()) {
                commands.push(cmd);
            }
        }

        // 4. User commands (~/.claude/commands/)
        let user_commands_dir = home.join(".claude").join("commands");
        for cmd in scan_commands_dir(&user_commands_dir, SlashCommandSource::UserCommand) {
            if seen_names.insert(cmd.name.clone()) {
                commands.push(cmd);
            }
        }
    }

    // 5. Project skills ({session_cwd}/.claude/skills/)
    if let Some(cwd) = session_cwd {
        let project_root = Path::new(cwd);
        let project_skills_dir = project_root.join(".claude").join("skills");
        for cmd in scan_skills_dir(&project_skills_dir, SlashCommandSource::ProjectSkill) {
            if seen_names.insert(cmd.name.clone()) {
                commands.push(cmd);
            }
        }

        // 6. Project commands ({session_cwd}/.claude/commands/)
        let project_commands_dir = project_root.join(".claude").join("commands");
        for cmd in scan_commands_dir(&project_commands_dir, SlashCommandSource::ProjectCommand) {
            if seen_names.insert(cmd.name.clone()) {
                commands.push(cmd);
            }
        }
    }

    // Sort: source priority, then alphabetical
    commands.sort_by(|a, b| a.source.cmp(&b.source).then(a.name.cmp(&b.name)));

    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_frontmatter_valid() {
        let content = "---\nname: test\ndescription: A test skill\n---\n\nBody content";
        let fm = extract_frontmatter(content);
        assert!(fm.is_some());
        assert!(fm.unwrap().contains("name: test"));
    }

    #[test]
    fn test_extract_frontmatter_no_markers() {
        let content = "Just some text without frontmatter";
        assert!(extract_frontmatter(content).is_none());
    }

    #[test]
    fn test_extract_frontmatter_missing_close() {
        let content = "---\nname: test\nNo closing marker";
        assert!(extract_frontmatter(content).is_none());
    }

    #[test]
    fn test_parse_skill_file_basic() {
        let content = "---\nname: my-skill\ndescription: Does something cool\n---\n\nInstructions here";
        let cmd = parse_skill_file(content, "fallback", SlashCommandSource::UserSkill);
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert_eq!(cmd.name, "my-skill");
        assert_eq!(cmd.description, "Does something cool");
        assert_eq!(cmd.source, SlashCommandSource::UserSkill);
    }

    #[test]
    fn test_parse_skill_file_fallback_name() {
        let content = "---\ndescription: No name field\n---\n\nBody";
        let cmd = parse_skill_file(content, "dir-name", SlashCommandSource::ProjectSkill);
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "dir-name");
    }

    #[test]
    fn test_parse_skill_file_not_user_invocable() {
        let content = "---\nname: hidden\nuser-invocable: false\n---\n\nBody";
        let cmd = parse_skill_file(content, "hidden", SlashCommandSource::UserSkill);
        assert!(cmd.is_none());
    }

    #[test]
    fn test_parse_skill_file_with_argument_hint() {
        let content = "---\nname: fix-issue\nargument-hint: \"[issue-number]\"\ndescription: Fix a GitHub issue\n---\n\nBody";
        let cmd = parse_skill_file(content, "fix-issue", SlashCommandSource::UserSkill).unwrap();
        assert_eq!(cmd.argument_hint.as_deref(), Some("[issue-number]"));
    }

    #[test]
    fn test_parse_skill_file_description_with_colon() {
        let content = "---\nname: my-tool\ndescription: \"Use when: the user asks for help\"\n---\n\nBody";
        let cmd = parse_skill_file(content, "my-tool", SlashCommandSource::UserSkill).unwrap();
        assert_eq!(cmd.description, "Use when: the user asks for help");
    }

    #[test]
    fn test_parse_command_file() {
        let content = "# Review Code\n\nReview the current changes";
        let cmd = parse_command_file(content, "review", SlashCommandSource::UserCommand);
        assert_eq!(cmd.name, "review");
        assert_eq!(cmd.description, "Review Code");
    }

    #[test]
    fn test_builtin_commands_count() {
        assert_eq!(BUILTIN_COMMANDS.len(), 29);
    }

    #[test]
    fn test_scan_slash_commands_includes_builtins() {
        let commands = scan_slash_commands(None);
        assert!(commands.iter().any(|c| c.name == "compact"));
        assert!(commands.iter().any(|c| c.name == "help"));
        assert!(commands.iter().any(|c| c.name == "vim"));
    }

    #[test]
    fn test_scan_slash_commands_sorted() {
        let commands = scan_slash_commands(None);
        // All built-ins should come first
        let first_non_builtin = commands.iter().position(|c| c.source != SlashCommandSource::BuiltIn);
        if let Some(pos) = first_non_builtin {
            // Everything before should be built-in
            for c in &commands[..pos] {
                assert_eq!(c.source, SlashCommandSource::BuiltIn);
            }
        }
        // Built-ins should be alphabetically sorted
        let builtins: Vec<&str> = commands
            .iter()
            .filter(|c| c.source == SlashCommandSource::BuiltIn)
            .map(|c| c.name.as_str())
            .collect();
        let mut sorted = builtins.clone();
        sorted.sort();
        assert_eq!(builtins, sorted);
    }

    #[test]
    fn test_scan_skills_dir_with_tempdir() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Test skill\n---\n\nBody",
        ).unwrap();

        let commands = scan_skills_dir(tmp.path(), SlashCommandSource::UserSkill);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "my-skill");
    }

    #[test]
    fn test_scan_commands_dir_with_subdirs() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("review.md"), "# Review\n\nReview code").unwrap();
        let sub = tmp.path().join("frontend");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("component.md"), "# Component\n\nCreate component").unwrap();

        let commands = scan_commands_dir(tmp.path(), SlashCommandSource::UserCommand);
        assert_eq!(commands.len(), 2);
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"review"));
        assert!(names.contains(&"frontend:component"));
    }

    #[test]
    fn test_deduplication_builtin_wins() {
        // If a user skill has the same name as a built-in, built-in wins
        let commands = scan_slash_commands(None);
        let help_commands: Vec<_> = commands.iter().filter(|c| c.name == "help").collect();
        assert_eq!(help_commands.len(), 1);
        assert_eq!(help_commands[0].source, SlashCommandSource::BuiltIn);
    }
}
