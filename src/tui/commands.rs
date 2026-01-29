//! Slash command parsing and definitions

use crossterm::style::Stylize;

/// Available slash commands
#[derive(Debug)]
pub enum SlashCommand {
    Help,
    Quit,
    Clear,
    Model(Option<String>),
    Provider(Option<String>),
    Stats,
    Context(ContextAction),
    Status,
    Compact,
}

/// Context management sub-actions
#[derive(Debug)]
pub enum ContextAction {
    Add(String),
    Remove(String),
    List,
    Clear,
}

/// Parse a slash command from user input.
/// Returns None if the input is not a slash command.
pub fn parse_command(input: &str) -> Option<SlashCommand> {
    let input = input.trim();
    if !input.starts_with('/') {
        return None;
    }

    let parts: Vec<&str> = input.splitn(3, ' ').collect();
    let cmd = parts[0].to_lowercase();
    let arg1 = parts.get(1).map(|s| s.to_string());
    let arg2 = parts.get(2).map(|s| s.to_string());

    match cmd.as_str() {
        "/help" | "/h" | "/?" => Some(SlashCommand::Help),
        "/quit" | "/q" | "/exit" => Some(SlashCommand::Quit),
        "/clear" | "/cls" => Some(SlashCommand::Clear),
        "/model" => Some(SlashCommand::Model(arg1)),
        "/provider" => Some(SlashCommand::Provider(arg1)),
        "/stats" => Some(SlashCommand::Stats),
        "/status" => Some(SlashCommand::Status),
        "/compact" => Some(SlashCommand::Compact),
        "/context" | "/ctx" => {
            let action = arg1.as_deref().unwrap_or("list");
            match action {
                "add" | "a" => {
                    if let Some(path) = arg2 {
                        Some(SlashCommand::Context(ContextAction::Add(path)))
                    } else {
                        Some(SlashCommand::Context(ContextAction::List))
                    }
                }
                "remove" | "rm" | "r" => {
                    if let Some(name) = arg2 {
                        Some(SlashCommand::Context(ContextAction::Remove(name)))
                    } else {
                        Some(SlashCommand::Context(ContextAction::List))
                    }
                }
                "list" | "ls" | "l" => Some(SlashCommand::Context(ContextAction::List)),
                "clear" | "c" => Some(SlashCommand::Context(ContextAction::Clear)),
                _ => {
                    // Treat unknown subcommand as file path to add
                    Some(SlashCommand::Context(ContextAction::Add(action.to_string())))
                }
            }
        }
        _ => None,
    }
}

/// Render help text for all slash commands
pub fn render_help(
    renderer: &super::renderer::TerminalRenderer,
) {
    let cmd_color = renderer.command_color();
    let dim_color = renderer.dim_color();

    println!();
    renderer.render_system("Available commands:");
    println!();

    let commands = [
        ("/help", "Show this help message"),
        ("/quit", "Exit interactive mode"),
        ("/clear", "Clear the conversation history"),
        ("/model [name]", "Show or change the current model"),
        ("/provider [name]", "Show or change the provider"),
        ("/stats", "Show token usage statistics"),
        ("/status", "Show current provider and session status"),
        ("/compact", "Compact conversation history to save tokens"),
        ("/context add <file>", "Add a file to context"),
        ("/context remove <name>", "Remove a context file"),
        ("/context list", "List current context files"),
        ("/context clear", "Clear all context files"),
    ];

    for (cmd, desc) in &commands {
        println!(
            "  {:<25} {}",
            cmd.with(cmd_color),
            desc.with(dim_color)
        );
    }
    println!();
}
