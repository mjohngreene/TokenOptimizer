//! Terminal rendering with markdown support

use crossterm::style::{Color, Stylize};
use termimad::MadSkin;

use super::theme::Theme;

/// Terminal renderer with markdown and styled output
pub struct TerminalRenderer {
    theme: Theme,
    skin: MadSkin,
}

impl TerminalRenderer {
    pub fn new() -> Self {
        let theme = Theme::default();
        let skin = Self::build_skin(&theme);
        Self { theme, skin }
    }

    fn build_skin(theme: &Theme) -> MadSkin {
        let mut skin = MadSkin::default();
        skin.set_headers_fg(to_termimad_color(theme.title));
        skin.bold.set_fg(to_termimad_color(Color::White));
        skin.italic.set_fg(to_termimad_color(Color::DarkYellow));
        skin.inline_code.set_fg(to_termimad_color(Color::Green));
        skin.code_block.set_fg(to_termimad_color(Color::Green));
        skin
    }

    /// Render the welcome banner
    pub fn render_banner(&self, version: &str, provider: &str, model: &str) {
        self.render_banner_pipeline(version, provider, model, None, None);
    }

    /// Render the welcome banner with full pipeline info
    pub fn render_banner_pipeline(
        &self,
        version: &str,
        provider: &str,
        model: &str,
        local_info: Option<&str>,
        fallback_info: Option<&str>,
    ) {
        println!();
        println!(
            "{}",
            "  TokenOptimizer Interactive Mode"
                .with(self.theme.title)
        );
        println!(
            "  {} {}",
            "v".with(self.theme.dim),
            version.with(self.theme.dim)
        );

        // Build pipeline display
        let mut pipeline_parts = Vec::new();
        if let Some(local) = local_info {
            pipeline_parts.push(format!("Local ({})", local));
        }
        pipeline_parts.push(format!("{} ({})", provider, model));
        if let Some(fb) = fallback_info {
            pipeline_parts.push(format!("{} (fallback)", fb));
        }
        let pipeline = pipeline_parts.join(" -> ");

        println!(
            "  {} {}",
            "Pipeline:".with(self.theme.dim),
            pipeline.with(self.theme.stats),
        );

        if local_info.is_none() && fallback_info.is_none() {
            // Simple mode, no pipeline extras
        } else if local_info.is_none() {
            println!(
                "  {}",
                "Local preprocessing: unavailable".with(self.theme.dim)
            );
        }

        println!(
            "  {}",
            "Type /help for commands, /quit to exit"
                .with(self.theme.dim)
        );
        println!();
    }

    /// Render a streaming text delta (raw, no markdown processing)
    pub fn render_delta(&self, text: &str) {
        use std::io::Write;
        print!("{}", text.with(self.theme.assistant));
        let _ = std::io::stdout().flush();
    }

    /// Render a complete response with markdown formatting
    pub fn render_markdown(&self, content: &str) {
        // Only re-render with markdown if content has markdown elements
        if has_markdown_elements(content) {
            // Clear the raw streamed output and re-render
            // Move to start of the response area
            println!(); // Ensure we're on a new line
            self.skin.print_text(content);
        } else {
            // Content was already printed during streaming, just add newline
            println!();
        }
    }

    /// Render the token usage line after a response
    pub fn render_usage_line(
        &self,
        prompt_tokens: u32,
        completion_tokens: u32,
        model: &str,
        cached: bool,
    ) {
        let cache_info = if cached { " (cached)" } else { "" };
        println!(
            "\n  {} {} prompt + {} completion{} [{}]",
            "\u{2022}".with(self.theme.dim),
            format!("{}", prompt_tokens).with(self.theme.stats),
            format!("{}", completion_tokens).with(self.theme.stats),
            cache_info.with(self.theme.dim),
            model.with(self.theme.dim),
        );
        println!();
    }

    /// Render a system message
    pub fn render_system(&self, msg: &str) {
        println!(
            "  {} {}",
            "\u{25b6}".with(self.theme.system),
            msg.with(self.theme.system)
        );
    }

    /// Render an error message
    pub fn render_error(&self, msg: &str) {
        println!(
            "  {} {}",
            "\u{2717}".with(self.theme.error),
            msg.with(self.theme.error)
        );
    }

    /// Render a success message
    pub fn render_success(&self, msg: &str) {
        println!(
            "  {} {}",
            "\u{2713}".with(self.theme.success),
            msg.with(self.theme.success)
        );
    }

    /// Render info text
    pub fn render_info(&self, msg: &str) {
        println!("  {}", msg.with(self.theme.dim));
    }

    /// Print styled prompt and return color for interactive use
    pub fn prompt_color(&self) -> Color {
        self.theme.prompt
    }

    pub fn command_color(&self) -> Color {
        self.theme.command
    }

    pub fn dim_color(&self) -> Color {
        self.theme.dim
    }

    pub fn stats_color(&self) -> Color {
        self.theme.stats
    }
}

/// Check if content has markdown elements worth re-rendering
fn has_markdown_elements(content: &str) -> bool {
    content.contains("```")
        || content.contains("## ")
        || content.contains("# ")
        || content.contains("**")
        || content.contains("| ")
        || content.contains("- [")
}

/// Convert crossterm Color to termimad color
fn to_termimad_color(color: Color) -> termimad::crossterm::style::Color {
    // termimad re-exports crossterm, so these types are compatible
    match color {
        Color::Black => termimad::crossterm::style::Color::Black,
        Color::DarkGrey => termimad::crossterm::style::Color::DarkGrey,
        Color::Red => termimad::crossterm::style::Color::Red,
        Color::DarkRed => termimad::crossterm::style::Color::DarkRed,
        Color::Green => termimad::crossterm::style::Color::Green,
        Color::DarkGreen => termimad::crossterm::style::Color::DarkGreen,
        Color::Yellow => termimad::crossterm::style::Color::Yellow,
        Color::DarkYellow => termimad::crossterm::style::Color::DarkYellow,
        Color::Blue => termimad::crossterm::style::Color::Blue,
        Color::DarkBlue => termimad::crossterm::style::Color::DarkBlue,
        Color::Magenta => termimad::crossterm::style::Color::Magenta,
        Color::DarkMagenta => termimad::crossterm::style::Color::DarkMagenta,
        Color::Cyan => termimad::crossterm::style::Color::Cyan,
        Color::DarkCyan => termimad::crossterm::style::Color::DarkCyan,
        Color::White => termimad::crossterm::style::Color::White,
        Color::Grey => termimad::crossterm::style::Color::Grey,
        _ => termimad::crossterm::style::Color::Reset,
    }
}
