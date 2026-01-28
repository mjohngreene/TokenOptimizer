//! Metrics and tracking for token usage

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Token usage metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenMetrics {
    /// Total input tokens used
    pub total_input_tokens: u64,
    /// Total output tokens used
    pub total_output_tokens: u64,
    /// Tokens saved through optimization
    pub tokens_saved: u64,
    /// Number of API requests made
    pub request_count: u64,
    /// Total estimated cost (USD)
    pub estimated_cost: f64,
    /// Per-session metrics
    #[serde(skip)]
    pub sessions: HashMap<String, SessionMetrics>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionMetrics {
    pub session_id: String,
    pub start_time: Option<Instant>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tokens_saved: u64,
    pub request_count: u64,
    pub optimizations_applied: Vec<String>,
}

impl TokenMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_request(
        &mut self,
        input_tokens: u32,
        output_tokens: u32,
        tokens_saved: u32,
        cost: Option<f64>,
    ) {
        self.total_input_tokens += input_tokens as u64;
        self.total_output_tokens += output_tokens as u64;
        self.tokens_saved += tokens_saved as u64;
        self.request_count += 1;

        if let Some(c) = cost {
            self.estimated_cost += c;
        }
    }

    pub fn compression_ratio(&self) -> f64 {
        let total_before = self.total_input_tokens + self.tokens_saved;
        if total_before == 0 {
            return 1.0;
        }
        self.total_input_tokens as f64 / total_before as f64
    }

    pub fn average_tokens_per_request(&self) -> f64 {
        if self.request_count == 0 {
            return 0.0;
        }
        (self.total_input_tokens + self.total_output_tokens) as f64 / self.request_count as f64
    }

    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens + self.total_output_tokens
    }

    pub fn start_session(&mut self, session_id: &str) {
        self.sessions.insert(
            session_id.to_string(),
            SessionMetrics {
                session_id: session_id.to_string(),
                start_time: Some(Instant::now()),
                ..Default::default()
            },
        );
    }

    pub fn end_session(&mut self, session_id: &str) -> Option<SessionMetrics> {
        self.sessions.remove(session_id)
    }

    pub fn record_session_request(
        &mut self,
        session_id: &str,
        input_tokens: u32,
        output_tokens: u32,
        tokens_saved: u32,
    ) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.input_tokens += input_tokens as u64;
            session.output_tokens += output_tokens as u64;
            session.tokens_saved += tokens_saved as u64;
            session.request_count += 1;
        }

        self.record_request(input_tokens, output_tokens, tokens_saved, None);
    }
}

/// Thread-safe metrics tracker
#[derive(Clone)]
pub struct MetricsTracker {
    inner: Arc<Mutex<TokenMetrics>>,
}

impl MetricsTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TokenMetrics::new())),
        }
    }

    pub fn record_request(
        &self,
        input_tokens: u32,
        output_tokens: u32,
        tokens_saved: u32,
        cost: Option<f64>,
    ) {
        if let Ok(mut metrics) = self.inner.lock() {
            metrics.record_request(input_tokens, output_tokens, tokens_saved, cost);
        }
    }

    pub fn get_metrics(&self) -> TokenMetrics {
        self.inner
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    pub fn start_session(&self, session_id: &str) {
        if let Ok(mut metrics) = self.inner.lock() {
            metrics.start_session(session_id);
        }
    }

    pub fn end_session(&self, session_id: &str) -> Option<SessionMetrics> {
        self.inner.lock().ok()?.end_session(session_id)
    }

    pub fn record_session_request(
        &self,
        session_id: &str,
        input_tokens: u32,
        output_tokens: u32,
        tokens_saved: u32,
    ) {
        if let Ok(mut metrics) = self.inner.lock() {
            metrics.record_session_request(session_id, input_tokens, output_tokens, tokens_saved);
        }
    }

    pub fn summary(&self) -> MetricsSummary {
        let metrics = self.get_metrics();
        MetricsSummary {
            total_tokens: metrics.total_tokens(),
            tokens_saved: metrics.tokens_saved,
            compression_ratio: metrics.compression_ratio(),
            request_count: metrics.request_count,
            estimated_cost: metrics.estimated_cost,
            avg_tokens_per_request: metrics.average_tokens_per_request(),
        }
    }
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSummary {
    pub total_tokens: u64,
    pub tokens_saved: u64,
    pub compression_ratio: f64,
    pub request_count: u64,
    pub estimated_cost: f64,
    pub avg_tokens_per_request: f64,
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Token Metrics Summary ===")?;
        writeln!(f, "Total tokens used: {}", self.total_tokens)?;
        writeln!(f, "Tokens saved: {}", self.tokens_saved)?;
        writeln!(f, "Compression ratio: {:.2}%", self.compression_ratio * 100.0)?;
        writeln!(f, "Total requests: {}", self.request_count)?;
        writeln!(f, "Avg tokens/request: {:.1}", self.avg_tokens_per_request)?;
        writeln!(f, "Estimated cost: ${:.4}", self.estimated_cost)?;
        Ok(())
    }
}

/// Benchmark results for comparing optimization strategies
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkResult {
    pub strategy_name: String,
    pub original_tokens: usize,
    pub optimized_tokens: usize,
    pub compression_ratio: f64,
    pub quality_score: Option<f64>,
    pub processing_time: Duration,
}

impl BenchmarkResult {
    pub fn tokens_saved(&self) -> usize {
        self.original_tokens.saturating_sub(self.optimized_tokens)
    }
}
