//! Input prompt with history support

use crossterm::style::Stylize;
use std::io::{self, BufRead, Write};

/// Handles user input with styled prompt and input history
pub struct PromptHandler {
    history: Vec<String>,
}

impl PromptHandler {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }

    /// Display the prompt and read a line of input.
    /// Returns None on EOF (Ctrl+D).
    pub fn read_line(&mut self, prompt_color: crossterm::style::Color) -> Option<String> {
        print!("{} ", ">".with(prompt_color));
        io::stdout().flush().ok()?;

        let stdin = io::stdin();
        let mut line = String::new();

        match stdin.lock().read_line(&mut line) {
            Ok(0) => None, // EOF
            Ok(_) => {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    self.history.push(trimmed.clone());
                }
                Some(trimmed)
            }
            Err(_) => None,
        }
    }

    /// Get input history
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Get the number of inputs in history
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
}
