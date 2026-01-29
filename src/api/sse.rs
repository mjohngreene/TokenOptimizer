//! Server-Sent Events (SSE) parser for streaming API responses
//!
//! Handles three formats:
//! - OpenAI/Venice: `data: {"choices":[{"delta":{"content":"..."}}]}`
//! - Anthropic: `event: content_block_delta` / `data: {"delta":{"text":"..."}}`
//! - Ollama: line-delimited JSON `{"response":"..."}`

use super::streaming::StreamChunk;
use super::TokenUsage;
use serde_json::Value;

/// The format of SSE events from the provider
#[derive(Debug, Clone, Copy)]
pub enum SseFormat {
    /// OpenAI-compatible format (also used by Venice)
    OpenAI,
    /// Anthropic format with event types
    Anthropic,
    /// Ollama line-delimited JSON
    Ollama,
}

/// Parse a single SSE line or data payload into a StreamChunk.
/// Returns None if the line should be skipped (comments, empty lines, event types).
pub fn parse_sse_line(line: &str, format: SseFormat) -> Option<StreamChunk> {
    let line = line.trim();

    // Skip empty lines and SSE comments
    if line.is_empty() || line.starts_with(':') {
        return None;
    }

    match format {
        SseFormat::OpenAI => parse_openai_sse(line),
        SseFormat::Anthropic => parse_anthropic_sse(line),
        SseFormat::Ollama => parse_ollama_line(line),
    }
}

fn parse_openai_sse(line: &str) -> Option<StreamChunk> {
    // Only process data lines
    let data = line.strip_prefix("data: ")?;

    // Check for stream end
    if data.trim() == "[DONE]" {
        return Some(StreamChunk::Done(TokenUsage::default()));
    }

    // Parse JSON
    let json: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => return Some(StreamChunk::Error(format!("JSON parse error: {}", e))),
    };

    // Check for content delta
    if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
        if !content.is_empty() {
            return Some(StreamChunk::TextDelta(content.to_string()));
        }
    }

    // Check for finish_reason
    if let Some(reason) = json["choices"][0]["finish_reason"].as_str() {
        if reason == "stop" || reason == "length" {
            let usage = if let Some(usage_obj) = json.get("usage") {
                TokenUsage::new(
                    usage_obj["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                    usage_obj["completion_tokens"].as_u64().unwrap_or(0) as u32,
                )
            } else {
                TokenUsage::default()
            };
            return Some(StreamChunk::Done(usage));
        }
    }

    None
}

fn parse_anthropic_sse(line: &str) -> Option<StreamChunk> {
    // Skip event type lines (we process based on data content)
    if line.starts_with("event:") {
        return None;
    }

    let data = line.strip_prefix("data: ")?;

    let json: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => return Some(StreamChunk::Error(format!("JSON parse error: {}", e))),
    };

    // Check event type in the data
    let event_type = json["type"].as_str().unwrap_or("");

    match event_type {
        "content_block_delta" => {
            if let Some(text) = json["delta"]["text"].as_str() {
                if !text.is_empty() {
                    return Some(StreamChunk::TextDelta(text.to_string()));
                }
            }
        }
        "message_delta" => {
            // Final message with usage info
            let usage = if let Some(usage_obj) = json.get("usage") {
                TokenUsage::new(
                    usage_obj["input_tokens"].as_u64().unwrap_or(0) as u32,
                    usage_obj["output_tokens"].as_u64().unwrap_or(0) as u32,
                )
            } else {
                TokenUsage::default()
            };
            return Some(StreamChunk::Done(usage));
        }
        "message_stop" => {
            return Some(StreamChunk::Done(TokenUsage::default()));
        }
        "error" => {
            let msg = json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Some(StreamChunk::Error(msg.to_string()));
        }
        _ => {}
    }

    None
}

fn parse_ollama_line(line: &str) -> Option<StreamChunk> {
    let json: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Ollama chat format
    if let Some(content) = json["message"]["content"].as_str() {
        if !content.is_empty() {
            return Some(StreamChunk::TextDelta(content.to_string()));
        }
    }

    // Ollama generate format
    if let Some(response) = json["response"].as_str() {
        if !response.is_empty() {
            return Some(StreamChunk::TextDelta(response.to_string()));
        }
    }

    // Check for done signal
    if json["done"].as_bool() == Some(true) {
        let usage = TokenUsage::new(
            json["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
            json["eval_count"].as_u64().unwrap_or(0) as u32,
        );
        return Some(StreamChunk::Done(usage));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_text_delta() {
        let line = r#"data: {"choices":[{"delta":{"content":"Hello"},"index":0}]}"#;
        match parse_sse_line(line, SseFormat::OpenAI) {
            Some(StreamChunk::TextDelta(text)) => assert_eq!(text, "Hello"),
            other => panic!("Expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_openai_done() {
        let line = "data: [DONE]";
        match parse_sse_line(line, SseFormat::OpenAI) {
            Some(StreamChunk::Done(_)) => {}
            other => panic!("Expected Done, got {:?}", other),
        }
    }

    #[test]
    fn test_anthropic_text_delta() {
        let line = r#"data: {"type":"content_block_delta","delta":{"text":"world"}}"#;
        match parse_sse_line(line, SseFormat::Anthropic) {
            Some(StreamChunk::TextDelta(text)) => assert_eq!(text, "world"),
            other => panic!("Expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_anthropic_event_line_skipped() {
        let line = "event: content_block_delta";
        assert!(parse_sse_line(line, SseFormat::Anthropic).is_none());
    }

    #[test]
    fn test_ollama_response() {
        let line = r#"{"message":{"content":"Hi"},"done":false}"#;
        match parse_sse_line(line, SseFormat::Ollama) {
            Some(StreamChunk::TextDelta(text)) => assert_eq!(text, "Hi"),
            other => panic!("Expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_ollama_done() {
        let line = r#"{"done":true,"prompt_eval_count":10,"eval_count":20}"#;
        match parse_sse_line(line, SseFormat::Ollama) {
            Some(StreamChunk::Done(usage)) => {
                assert_eq!(usage.prompt_tokens, 10);
                assert_eq!(usage.completion_tokens, 20);
            }
            other => panic!("Expected Done, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_line_skipped() {
        assert!(parse_sse_line("", SseFormat::OpenAI).is_none());
        assert!(parse_sse_line("  ", SseFormat::Anthropic).is_none());
    }

    #[test]
    fn test_comment_skipped() {
        assert!(parse_sse_line(": keep-alive", SseFormat::OpenAI).is_none());
    }
}
