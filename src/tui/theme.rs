//! Terminal theme and color definitions

use crossterm::style::Color;

/// Theme colors for the interactive shell
pub struct Theme {
    /// Color for the user prompt symbol
    pub prompt: Color,
    /// Color for assistant response text
    pub assistant: Color,
    /// Color for system messages
    pub system: Color,
    /// Color for error messages
    pub error: Color,
    /// Color for dim/secondary info
    pub dim: Color,
    /// Color for success messages
    pub success: Color,
    /// Color for the banner/title
    pub title: Color,
    /// Color for usage/stats numbers
    pub stats: Color,
    /// Color for slash command names
    pub command: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            prompt: Color::Cyan,
            assistant: Color::White,
            system: Color::DarkYellow,
            error: Color::Red,
            dim: Color::DarkGrey,
            success: Color::Green,
            title: Color::Magenta,
            stats: Color::Blue,
            command: Color::Yellow,
        }
    }
}
