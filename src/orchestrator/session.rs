//! Session management for agent orchestration
//!
//! Handles stateful sessions across provider transitions

use crate::api::{ApiRequest, ApiResponse, ContextItem};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Maximum messages to retain in history
    pub max_history: usize,
    /// Whether to include history in handoff
    pub include_history_in_handoff: bool,
    /// Compress history before handoff
    pub compress_history: bool,
    /// Session timeout in seconds
    pub timeout_secs: Option<u64>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_history: 20,
            include_history_in_handoff: true,
            compress_history: true,
            timeout_secs: Some(3600), // 1 hour
        }
    }
}

/// Current state of a session
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Session is active with primary provider
    Active,
    /// Session handed off to fallback
    HandedOff,
    /// Session completed
    Completed,
    /// Session expired
    Expired,
}

/// A conversation turn in the session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    /// The request that was sent
    pub request_summary: String,
    /// The response received
    pub response_summary: String,
    /// Provider that handled this turn
    pub provider: String,
    /// Token usage for this turn
    pub tokens_used: u32,
    /// Timestamp
    #[serde(skip)]
    pub timestamp: Option<Instant>,
}

/// Manages a coding session across provider transitions
#[derive(Debug)]
pub struct Session {
    /// Unique session identifier
    pub id: String,
    /// Session configuration
    config: SessionConfig,
    /// Current state
    state: SessionState,
    /// Conversation history
    history: Vec<Turn>,
    /// Accumulated context
    context: Vec<ContextItem>,
    /// Provider that started the session
    initial_provider: String,
    /// Current provider
    current_provider: String,
    /// Session start time
    started_at: Instant,
    /// Total tokens used in session
    total_tokens: u64,
    /// Total cost in session
    total_cost: f64,
}

impl Session {
    pub fn new(id: String, config: SessionConfig, initial_provider: String) -> Self {
        Self {
            id,
            config,
            state: SessionState::Active,
            history: Vec::new(),
            context: Vec::new(),
            initial_provider: initial_provider.clone(),
            current_provider: initial_provider,
            started_at: Instant::now(),
            total_tokens: 0,
            total_cost: 0.0,
        }
    }

    /// Record a turn in the session
    pub fn record_turn(&mut self, request: &ApiRequest, response: &ApiResponse, provider: &str) {
        // Summarize request (truncate if too long)
        let request_summary = if request.task.len() > 200 {
            format!("{}...", &request.task[..200])
        } else {
            request.task.clone()
        };

        // Summarize response
        let response_summary = if response.content.len() > 500 {
            format!("{}...", &response.content[..500])
        } else {
            response.content.clone()
        };

        let turn = Turn {
            request_summary,
            response_summary,
            provider: provider.to_string(),
            tokens_used: response.usage.total_tokens,
            timestamp: Some(Instant::now()),
        };

        self.history.push(turn);
        self.total_tokens += response.usage.total_tokens as u64;

        if let Some(cost) = response.usage.estimated_cost_usd {
            self.total_cost += cost;
        }

        // Trim history if needed
        while self.history.len() > self.config.max_history {
            self.history.remove(0);
        }
    }

    /// Add context to the session
    pub fn add_context(&mut self, item: ContextItem) {
        // Check for duplicates
        if !self.context.iter().any(|c| c.name == item.name) {
            self.context.push(item);
        }
    }

    /// Get context for handoff
    pub fn get_handoff_context(&self) -> String {
        let mut context = String::new();

        if self.config.include_history_in_handoff && !self.history.is_empty() {
            context.push_str("## Previous Conversation Summary\n\n");

            for (i, turn) in self.history.iter().enumerate() {
                if self.config.compress_history {
                    // Only include key turns
                    if i == 0 || i == self.history.len() - 1 || i % 3 == 0 {
                        context.push_str(&format!(
                            "**Turn {} ({}):**\n- Request: {}\n- Response: {}\n\n",
                            i + 1,
                            turn.provider,
                            truncate(&turn.request_summary, 100),
                            truncate(&turn.response_summary, 200)
                        ));
                    }
                } else {
                    context.push_str(&format!(
                        "**Turn {} ({}):**\n- Request: {}\n- Response: {}\n\n",
                        i + 1,
                        turn.provider,
                        turn.request_summary,
                        turn.response_summary
                    ));
                }
            }
        }

        context
    }

    /// Mark session as handed off
    pub fn handoff(&mut self, new_provider: &str) {
        self.state = SessionState::HandedOff;
        self.current_provider = new_provider.to_string();
    }

    /// Complete the session
    pub fn complete(&mut self) {
        self.state = SessionState::Completed;
    }

    /// Check if session has expired
    pub fn is_expired(&self) -> bool {
        if let Some(timeout) = self.config.timeout_secs {
            self.started_at.elapsed().as_secs() > timeout
        } else {
            false
        }
    }

    /// Get session state
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// Get session statistics
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            id: self.id.clone(),
            state: self.state.clone(),
            turns: self.history.len(),
            total_tokens: self.total_tokens,
            total_cost: self.total_cost,
            duration_secs: self.started_at.elapsed().as_secs(),
            initial_provider: self.initial_provider.clone(),
            current_provider: self.current_provider.clone(),
            context_items: self.context.len(),
        }
    }

    /// Get history for display
    pub fn history(&self) -> &[Turn] {
        &self.history
    }

    /// Get current provider
    pub fn current_provider(&self) -> &str {
        &self.current_provider
    }
}

/// Session statistics
#[derive(Debug, Clone, Serialize)]
pub struct SessionStats {
    pub id: String,
    pub state: SessionState,
    pub turns: usize,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub duration_secs: u64,
    pub initial_provider: String,
    pub current_provider: String,
    pub context_items: usize,
}

impl std::fmt::Display for SessionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Session: {} ===", self.id)?;
        writeln!(f, "State: {:?}", self.state)?;
        writeln!(f, "Turns: {}", self.turns)?;
        writeln!(f, "Total tokens: {}", self.total_tokens)?;
        writeln!(f, "Total cost: ${:.4}", self.total_cost)?;
        writeln!(f, "Duration: {}s", self.duration_secs)?;
        writeln!(f, "Provider: {} -> {}", self.initial_provider, self.current_provider)?;
        writeln!(f, "Context items: {}", self.context_items)?;
        Ok(())
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new(
            "test-1".to_string(),
            SessionConfig::default(),
            "Venice".to_string(),
        );

        assert_eq!(session.state(), &SessionState::Active);
        assert_eq!(session.current_provider(), "Venice");
    }

    #[test]
    fn test_session_handoff() {
        let mut session = Session::new(
            "test-2".to_string(),
            SessionConfig::default(),
            "Venice".to_string(),
        );

        session.handoff("Claude Code");

        assert_eq!(session.state(), &SessionState::HandedOff);
        assert_eq!(session.current_provider(), "Claude Code");
    }
}
