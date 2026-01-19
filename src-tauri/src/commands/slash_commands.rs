use anyhow::{Context, Result};
use dirs;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Represents a custom slash command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    /// Unique identifier for the command (derived from file path)
    pub id: String,
    /// Command name (without prefix)
    pub name: String,
    /// Full command with prefix (e.g., "/project:optimize")
    pub full_command: String,
    /// Command scope: "project" or "user"
    pub scope: String,
    /// Optional namespace (e.g., "frontend" in "/project:frontend:component")
    pub namespace: Option<String>,
    /// Path to the markdown file
    pub file_path: String,
    /// Command content (markdown body)
    pub content: String,
    /// Optional description from frontmatter
    pub description: Option<String>,
    /// Allowed tools from frontmatter
    pub allowed_tools: Vec<String>,
    /// Whether the command has bash commands (!)
    pub has_bash_commands: bool,
    /// Whether the command has file references (@)
    pub has_file_references: bool,
    /// Whether the command uses $ARGUMENTS placeholder
    pub accepts_arguments: bool,
}

/// YAML frontmatter structure
#[derive(Debug, Deserialize)]
struct CommandFrontmatter {
    #[serde(rename = "allowed-tools")]
    allowed_tools: Option<Vec<String>>,
    description: Option<String>,
}

/// Parse a markdown file with optional YAML frontmatter
fn parse_markdown_with_frontmatter(content: &str) -> Result<(Option<CommandFrontmatter>, String)> {
    let lines: Vec<&str> = content.lines().collect();
    
    // Check if the file starts with YAML frontmatter
    if lines.is_empty() || lines[0] != "---" {
        // No frontmatter
        return Ok((None, content.to_string()));
    }
    
    // Find the end of frontmatter
    let mut frontmatter_end = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if *line == "---" {
            frontmatter_end = Some(i);
            break;
        }
    }
    
    if let Some(end) = frontmatter_end {
        // Extract frontmatter
        let frontmatter_content = lines[1..end].join("\n");
        let body_content = lines[(end + 1)..].join("\n");
        
        // Parse YAML
        match serde_yaml::from_str::<CommandFrontmatter>(&frontmatter_content) {
            Ok(frontmatter) => Ok((Some(frontmatter), body_content)),
            Err(e) => {
                debug!("Failed to parse frontmatter: {}", e);
                // Return full content if frontmatter parsing fails
                Ok((None, content.to_string()))
            }
        }
    } else {
        // Malformed frontmatter, treat as regular content
        Ok((None, content.to_string()))
    }
}

/// Extract command name and namespace from file path
fn extract_command_info(file_path: &Path, base_path: &Path) -> Result<(String, Option<String>), String> {
    let relative_path = file_path
        .strip_prefix(base_path)
        .map_err(|e| format!("Failed to get relative path: {}", e))?;
    
    // Remove .md extension
    let path_without_ext = relative_path
        .with_extension("")
        .to_string_lossy()
        .to_string();
    
    // Split into components
    let components: Vec<&str> = path_without_ext.split('/').collect();
    
    if components.is_empty() {
        return Err("Invalid command path".to_string());
    }
    
    if components.len() == 1 {
        // No namespace
        Ok((components[0].to_string(), None))
    } else {
        // Last component is the command name, rest is namespace
        let command_name = components.last().ok_or("Invalid command path".to_string())?.to_string();
        let namespace = components[..components.len() - 1].join(":");
        Ok((command_name, Some(namespace)))
    }
}

/// Load a single command from a markdown file
fn load_command_from_file(
    file_path: &Path,
    base_path: &Path,
    scope: &str,
) -> Result<SlashCommand> {
    debug!("Loading command from: {:?}", file_path);
    
    // Read file content
    let content = fs::read_to_string(file_path)
        .context("Failed to read command file")?;
    
    // Parse frontmatter
    let (frontmatter, body) = parse_markdown_with_frontmatter(&content)?;
    
    // Extract command info
    let (name, namespace) = extract_command_info(file_path, base_path)
        .map_err(|e| anyhow::anyhow!("Failed to extract command info: {}", e))?;
    
    // Build full command (no scope prefix, just /command or /namespace:command)
    let full_command = match &namespace {
        Some(ns) => format!("/{ns}:{name}"),
        None => format!("/{name}"),
    };
    
    // Generate unique ID
    let id = format!("{}-{}", scope, file_path.to_string_lossy().replace('/', "-"));
    
    // Check for special content
    let has_bash_commands = body.contains("!`");
    let has_file_references = body.contains('@');
    let accepts_arguments = body.contains("$ARGUMENTS");
    
    // Extract metadata from frontmatter
    let (description, allowed_tools) = if let Some(fm) = frontmatter {
        (fm.description, fm.allowed_tools.unwrap_or_default())
    } else {
        (None, Vec::new())
    };
    
    Ok(SlashCommand {
        id,
        name,
        full_command,
        scope: scope.to_string(),
        namespace,
        file_path: file_path.to_string_lossy().to_string(),
        content: body,
        description,
        allowed_tools,
        has_bash_commands,
        has_file_references,
        accepts_arguments,
    })
}

/// Recursively find all markdown files in a directory
fn find_markdown_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        // Skip hidden files/directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }
        
        if path.is_dir() {
            find_markdown_files(&path, files)?;
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" {
                    files.push(path);
                }
            }
        }
    }
    
    Ok(())
}

/// Create default/built-in slash commands
fn create_default_commands() -> Vec<SlashCommand> {
    vec![
        SlashCommand {
            id: "default-add-dir".to_string(),
            name: "add-dir".to_string(),
            full_command: "/add-dir".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Add additional working directories".to_string(),
            description: Some("Add additional working directories to the current session".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-init".to_string(),
            name: "init".to_string(),
            full_command: "/init".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Initialize project with CLAUDE.md guide".to_string(),
            description: Some("Initialize project with CLAUDE.md guide and setup files".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-review".to_string(),
            name: "review".to_string(),
            full_command: "/review".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Request code review".to_string(),
            description: Some("Request a comprehensive code review of recent changes".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-commit".to_string(),
            name: "commit".to_string(),
            full_command: "/commit".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Create a git commit".to_string(),
            description: Some("Create a git commit with staged changes and a descriptive message".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-review-pr".to_string(),
            name: "review-pr".to_string(),
            full_command: "/review-pr".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Review a pull request".to_string(),
            description: Some("Review a GitHub pull request and provide feedback".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: true,
        },
        SlashCommand {
            id: "default-pr".to_string(),
            name: "pr".to_string(),
            full_command: "/pr".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Create a pull request".to_string(),
            description: Some("Create a GitHub pull request from the current branch".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-test".to_string(),
            name: "test".to_string(),
            full_command: "/test".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Run tests".to_string(),
            description: Some("Run the project's test suite and analyze results".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-fix".to_string(),
            name: "fix".to_string(),
            full_command: "/fix".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Fix errors or issues".to_string(),
            description: Some("Analyze and fix errors, bugs, or issues in the code".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-debug".to_string(),
            name: "debug".to_string(),
            full_command: "/debug".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Debug an issue".to_string(),
            description: Some("Help debug and diagnose issues in the code".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-explain".to_string(),
            name: "explain".to_string(),
            full_command: "/explain".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Explain code or concepts".to_string(),
            description: Some("Explain how code works or clarify technical concepts".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-refactor".to_string(),
            name: "refactor".to_string(),
            full_command: "/refactor".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Refactor code".to_string(),
            description: Some("Refactor code to improve structure, readability, or performance".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-optimize".to_string(),
            name: "optimize".to_string(),
            full_command: "/optimize".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Optimize code performance".to_string(),
            description: Some("Optimize code for better performance and efficiency".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-docs".to_string(),
            name: "docs".to_string(),
            full_command: "/docs".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Generate documentation".to_string(),
            description: Some("Generate or update code documentation and comments".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-security".to_string(),
            name: "security".to_string(),
            full_command: "/security".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Security audit".to_string(),
            description: Some("Perform a security audit and identify vulnerabilities".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-remember".to_string(),
            name: "remember".to_string(),
            full_command: "/remember".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Remember information".to_string(),
            description: Some("Store information for future reference in the session".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: true,
        },
        SlashCommand {
            id: "default-model".to_string(),
            name: "model".to_string(),
            full_command: "/model".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Switch AI model".to_string(),
            description: Some("Switch between different Claude AI models".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-clear".to_string(),
            name: "clear".to_string(),
            full_command: "/clear".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Clear conversation".to_string(),
            description: Some("Clear the current conversation history".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-help".to_string(),
            name: "help".to_string(),
            full_command: "/help".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Show help information".to_string(),
            description: Some("Display help information and available commands".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-usage".to_string(),
            name: "usage".to_string(),
            full_command: "/usage".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Show usage statistics".to_string(),
            description: Some("Display API usage statistics and costs".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-settings".to_string(),
            name: "settings".to_string(),
            full_command: "/settings".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Open settings".to_string(),
            description: Some("Open and manage Claude Code settings".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-agents".to_string(),
            name: "agents".to_string(),
            full_command: "/agents".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Manage custom AI subagents".to_string(),
            description: Some("Manage custom AI subagents for specialized tasks".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-bashes".to_string(),
            name: "bashes".to_string(),
            full_command: "/bashes".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "List and manage background tasks".to_string(),
            description: Some("List and manage background bash tasks".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-bug".to_string(),
            name: "bug".to_string(),
            full_command: "/bug".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Report bugs".to_string(),
            description: Some("Report bugs (sends conversation to Anthropic)".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-compact".to_string(),
            name: "compact".to_string(),
            full_command: "/compact".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Compact conversation".to_string(),
            description: Some("Compact conversation with optional focus instructions".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: true,
        },
        SlashCommand {
            id: "default-config".to_string(),
            name: "config".to_string(),
            full_command: "/config".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Open settings interface".to_string(),
            description: Some("Open the Settings interface (Config tab). Type to search and filter settings".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-context".to_string(),
            name: "context".to_string(),
            full_command: "/context".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Visualize context usage".to_string(),
            description: Some("Visualize current context usage as a colored grid".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-cost".to_string(),
            name: "cost".to_string(),
            full_command: "/cost".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Show token usage statistics".to_string(),
            description: Some("Show token usage statistics and cost tracking".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-doctor".to_string(),
            name: "doctor".to_string(),
            full_command: "/doctor".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Check installation health".to_string(),
            description: Some("Checks installation health and shows update information".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-exit".to_string(),
            name: "exit".to_string(),
            full_command: "/exit".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Exit the REPL".to_string(),
            description: Some("Exit the Claude Code REPL interface".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-export".to_string(),
            name: "export".to_string(),
            full_command: "/export".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Export conversation".to_string(),
            description: Some("Export the current conversation to a file or clipboard".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: true,
        },
        SlashCommand {
            id: "default-hooks".to_string(),
            name: "hooks".to_string(),
            full_command: "/hooks".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Manage hook configurations".to_string(),
            description: Some("Manage hook configurations for tool events".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-ide".to_string(),
            name: "ide".to_string(),
            full_command: "/ide".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Manage IDE integrations".to_string(),
            description: Some("Manage IDE integrations and show status".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-install-github-app".to_string(),
            name: "install-github-app".to_string(),
            full_command: "/install-github-app".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Set up Claude GitHub Actions".to_string(),
            description: Some("Set up Claude GitHub Actions for a repository".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-login".to_string(),
            name: "login".to_string(),
            full_command: "/login".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Switch Anthropic accounts".to_string(),
            description: Some("Switch between different Anthropic accounts".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-logout".to_string(),
            name: "logout".to_string(),
            full_command: "/logout".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Sign out from account".to_string(),
            description: Some("Sign out from your Anthropic account".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-mcp".to_string(),
            name: "mcp".to_string(),
            full_command: "/mcp".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Manage MCP server connections".to_string(),
            description: Some("Manage MCP server connections and OAuth authentication".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-memory".to_string(),
            name: "memory".to_string(),
            full_command: "/memory".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Edit CLAUDE.md memory files".to_string(),
            description: Some("Edit CLAUDE.md memory files for project context".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-output-style".to_string(),
            name: "output-style".to_string(),
            full_command: "/output-style".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Set output style".to_string(),
            description: Some("Set the output style directly or from a selection menu".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: true,
        },
        SlashCommand {
            id: "default-permissions".to_string(),
            name: "permissions".to_string(),
            full_command: "/permissions".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "View or update permissions".to_string(),
            description: Some("View or update tool and command permissions".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-plan".to_string(),
            name: "plan".to_string(),
            full_command: "/plan".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Enter plan mode".to_string(),
            description: Some("Enter plan mode directly from the prompt".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-plugin".to_string(),
            name: "plugin".to_string(),
            full_command: "/plugin".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Manage Claude Code plugins".to_string(),
            description: Some("Manage and configure Claude Code plugins".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-pr-comments".to_string(),
            name: "pr-comments".to_string(),
            full_command: "/pr-comments".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "View pull request comments".to_string(),
            description: Some("View and manage pull request comments".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-privacy-settings".to_string(),
            name: "privacy-settings".to_string(),
            full_command: "/privacy-settings".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "View and update privacy settings".to_string(),
            description: Some("View and update your privacy settings".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-release-notes".to_string(),
            name: "release-notes".to_string(),
            full_command: "/release-notes".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "View release notes".to_string(),
            description: Some("View Claude Code release notes and changelog".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-rename".to_string(),
            name: "rename".to_string(),
            full_command: "/rename".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Rename current session".to_string(),
            description: Some("Rename the current session for easier identification".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: true,
        },
        SlashCommand {
            id: "default-remote-env".to_string(),
            name: "remote-env".to_string(),
            full_command: "/remote-env".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Configure remote session environment".to_string(),
            description: Some("Configure remote session environment (claude.ai subscribers)".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-resume".to_string(),
            name: "resume".to_string(),
            full_command: "/resume".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Resume a conversation".to_string(),
            description: Some("Resume a conversation by ID or name, or open the session picker".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: true,
        },
        SlashCommand {
            id: "default-rewind".to_string(),
            name: "rewind".to_string(),
            full_command: "/rewind".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Rewind the conversation".to_string(),
            description: Some("Rewind the conversation and/or code to a previous state".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-sandbox".to_string(),
            name: "sandbox".to_string(),
            full_command: "/sandbox".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Enable sandboxed bash tool".to_string(),
            description: Some("Enable sandboxed bash tool with filesystem and network isolation".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-security-review".to_string(),
            name: "security-review".to_string(),
            full_command: "/security-review".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Complete security review".to_string(),
            description: Some("Complete a security review of pending changes on the current branch".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-stats".to_string(),
            name: "stats".to_string(),
            full_command: "/stats".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Visualize usage statistics".to_string(),
            description: Some("Visualize daily usage, session history, streaks, and model preferences".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-status".to_string(),
            name: "status".to_string(),
            full_command: "/status".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Open status interface".to_string(),
            description: Some("Open the Settings interface (Status tab) showing version, model, account, and connectivity".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-statusline".to_string(),
            name: "statusline".to_string(),
            full_command: "/statusline".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Set up status line UI".to_string(),
            description: Some("Set up Claude Code's status line UI configuration".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-teleport".to_string(),
            name: "teleport".to_string(),
            full_command: "/teleport".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Resume remote session".to_string(),
            description: Some("Resume a remote session from claude.ai by session ID, or open a picker (claude.ai subscribers)".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: true,
        },
        SlashCommand {
            id: "default-terminal-setup".to_string(),
            name: "terminal-setup".to_string(),
            full_command: "/terminal-setup".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Install terminal key bindings".to_string(),
            description: Some("Install Shift+Enter key binding for newlines (VS Code, Alacritty, Zed, Warp)".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-theme".to_string(),
            name: "theme".to_string(),
            full_command: "/theme".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Change color theme".to_string(),
            description: Some("Change the Claude Code color theme".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-todos".to_string(),
            name: "todos".to_string(),
            full_command: "/todos".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "List current TODO items".to_string(),
            description: Some("List and manage current TODO items in the session".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
        SlashCommand {
            id: "default-vim".to_string(),
            name: "vim".to_string(),
            full_command: "/vim".to_string(),
            scope: "default".to_string(),
            namespace: None,
            file_path: "".to_string(),
            content: "Enter vim mode".to_string(),
            description: Some("Enter vim mode for alternating insert and command modes".to_string()),
            allowed_tools: vec![],
            has_bash_commands: false,
            has_file_references: false,
            accepts_arguments: false,
        },
    ]
}

/// Discover all custom slash commands
#[tauri::command]
pub async fn slash_commands_list(
    project_path: Option<String>,
) -> Result<Vec<SlashCommand>, String> {
    info!("Discovering slash commands");
    let mut commands = Vec::new();
    
    // Add default commands
    commands.extend(create_default_commands());
    
    // Load project commands if project path is provided
    if let Some(proj_path) = project_path {
        let project_commands_dir = PathBuf::from(&proj_path).join(".claude").join("commands");
        if project_commands_dir.exists() {
            debug!("Scanning project commands at: {:?}", project_commands_dir);
            
            let mut md_files = Vec::new();
            if let Err(e) = find_markdown_files(&project_commands_dir, &mut md_files) {
                error!("Failed to find project command files: {}", e);
            } else {
                for file_path in md_files {
                    match load_command_from_file(&file_path, &project_commands_dir, "project") {
                        Ok(cmd) => {
                            debug!("Loaded project command: {}", cmd.full_command);
                            commands.push(cmd);
                        }
                        Err(e) => {
                            error!("Failed to load command from {:?}: {}", file_path, e);
                        }
                    }
                }
            }
        }
    }
    
    // Load user commands
    if let Some(home_dir) = dirs::home_dir() {
        let user_commands_dir = home_dir.join(".claude").join("commands");
        if user_commands_dir.exists() {
            debug!("Scanning user commands at: {:?}", user_commands_dir);
            
            let mut md_files = Vec::new();
            if let Err(e) = find_markdown_files(&user_commands_dir, &mut md_files) {
                error!("Failed to find user command files: {}", e);
            } else {
                for file_path in md_files {
                    match load_command_from_file(&file_path, &user_commands_dir, "user") {
                        Ok(cmd) => {
                            debug!("Loaded user command: {}", cmd.full_command);
                            commands.push(cmd);
                        }
                        Err(e) => {
                            error!("Failed to load command from {:?}: {}", file_path, e);
                        }
                    }
                }
            }
        }
    }
    
    info!("Found {} slash commands", commands.len());
    Ok(commands)
}

/// Get a single slash command by ID
#[tauri::command]
pub async fn slash_command_get(command_id: String) -> Result<SlashCommand, String> {
    debug!("Getting slash command: {}", command_id);
    
    // Parse the ID to determine scope and reconstruct file path
    let parts: Vec<&str> = command_id.split('-').collect();
    if parts.len() < 2 {
        return Err("Invalid command ID".to_string());
    }
    
    // The actual implementation would need to reconstruct the path and reload the command
    // For now, we'll list all commands and find the matching one
    let commands = slash_commands_list(None).await?;
    
    commands
        .into_iter()
        .find(|cmd| cmd.id == command_id)
        .ok_or_else(|| format!("Command not found: {}", command_id))
}

/// Create or update a slash command
#[tauri::command]
pub async fn slash_command_save(
    scope: String,
    name: String,
    namespace: Option<String>,
    content: String,
    description: Option<String>,
    allowed_tools: Vec<String>,
    project_path: Option<String>,
) -> Result<SlashCommand, String> {
    info!("Saving slash command: {} in scope: {}", name, scope);
    
    // Validate inputs
    if name.is_empty() {
        return Err("Command name cannot be empty".to_string());
    }
    
    if !["project", "user"].contains(&scope.as_str()) {
        return Err("Invalid scope. Must be 'project' or 'user'".to_string());
    }
    
    // Determine base directory
    let base_dir = if scope == "project" {
        if let Some(proj_path) = project_path {
            PathBuf::from(proj_path).join(".claude").join("commands")
        } else {
            return Err("Project path required for project scope".to_string());
        }
    } else {
        dirs::home_dir()
            .ok_or_else(|| "Could not find home directory".to_string())?
            .join(".claude")
            .join("commands")
    };
    
    // Build file path
    let mut file_path = base_dir.clone();
    if let Some(ns) = &namespace {
        for component in ns.split(':') {
            file_path = file_path.join(component);
        }
    }
    
    // Create directories if needed
    fs::create_dir_all(&file_path)
        .map_err(|e| format!("Failed to create directories: {}", e))?;
    
    // Add filename
    file_path = file_path.join(format!("{}.md", name));
    
    // Build content with frontmatter
    let mut full_content = String::new();
    
    // Add frontmatter if we have metadata
    if description.is_some() || !allowed_tools.is_empty() {
        full_content.push_str("---\n");
        
        if let Some(desc) = &description {
            full_content.push_str(&format!("description: {}\n", desc));
        }
        
        if !allowed_tools.is_empty() {
            full_content.push_str("allowed-tools:\n");
            for tool in &allowed_tools {
                full_content.push_str(&format!("  - {}\n", tool));
            }
        }
        
        full_content.push_str("---\n\n");
    }
    
    full_content.push_str(&content);
    
    // Write file
    fs::write(&file_path, &full_content)
        .map_err(|e| format!("Failed to write command file: {}", e))?;
    
    // Load and return the saved command
    load_command_from_file(&file_path, &base_dir, &scope)
        .map_err(|e| format!("Failed to load saved command: {}", e))
}

/// Delete a slash command
#[tauri::command]
pub async fn slash_command_delete(command_id: String, project_path: Option<String>) -> Result<String, String> {
    info!("Deleting slash command: {}", command_id);
    
    // First, we need to determine if this is a project command by parsing the ID
    let is_project_command = command_id.starts_with("project-");
    
    // If it's a project command and we don't have a project path, error out
    if is_project_command && project_path.is_none() {
        return Err("Project path required to delete project commands".to_string());
    }
    
    // List all commands (including project commands if applicable)
    let commands = slash_commands_list(project_path).await?;
    
    // Find the command by ID
    let command = commands
        .into_iter()
        .find(|cmd| cmd.id == command_id)
        .ok_or_else(|| format!("Command not found: {}", command_id))?;
    
    // Delete the file
    fs::remove_file(&command.file_path)
        .map_err(|e| format!("Failed to delete command file: {}", e))?;
    
    // Clean up empty directories
    if let Some(parent) = Path::new(&command.file_path).parent() {
        let _ = remove_empty_dirs(parent);
    }
    
    Ok(format!("Deleted command: {}", command.full_command))
}

/// Remove empty directories recursively
fn remove_empty_dirs(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    
    // Check if directory is empty
    let is_empty = fs::read_dir(dir)?.next().is_none();
    
    if is_empty {
        fs::remove_dir(dir)?;
        
        // Try to remove parent if it's also empty
        if let Some(parent) = dir.parent() {
            let _ = remove_empty_dirs(parent);
        }
    }
    
    Ok(())
}
