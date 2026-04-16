use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::npc::prompt_builder::LlmMessage;
use crate::npc::voice::pipeline::LlmProvider;

/// OpenAI chat-completion LLM provider with streaming support.
///
/// Supports `gpt-4o-mini` (fast, cheap) and `gpt-4o` (vision-capable).
/// Uses Server-Sent Events (SSE) streaming for low time-to-first-token.
pub struct OpenAiLlm {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl OpenAiLlm {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiLlm {
    async fn stream_chat(
        &self,
        system: &str,
        messages: &[LlmMessage],
    ) -> Result<mpsc::Receiver<String>, String> {
        // Build the messages array: system + conversation
        let mut api_messages = Vec::with_capacity(messages.len() + 1);
        api_messages.push(serde_json::json!({
            "role": "system",
            "content": system
        }));
        for msg in messages {
            api_messages.push(serde_json::json!({
                "role": msg.role,
                "content": msg.content
            }));
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": api_messages,
            "stream": true,
            "temperature": 0.7,
            "max_tokens": 512, // Keep NPC responses short
        });

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(format!("OpenAI API error {}: {}", status, body));
        }

        // Stream SSE chunks through an mpsc channel
        let (tx, rx) = mpsc::channel::<String>(64);
        let byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut stream = byte_stream;
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[z-ro:npc] OpenAI stream error: {}", e);
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE lines
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            return;
                        }

                        // Parse the SSE data as JSON
                        if let Ok(parsed) = serde_json::from_str::<SseChunk>(data) {
                            if let Some(choice) = parsed.choices.first() {
                                if let Some(ref content) = choice.delta.content {
                                    if tx.send(content.clone()).await.is_err() {
                                        return; // Receiver dropped
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    fn name(&self) -> &str {
        &self.model
    }
}

// ── SSE JSON types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SseChunk {
    choices: Vec<SseChoice>,
}

#[derive(Debug, Deserialize)]
struct SseChoice {
    delta: SseDelta,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    content: Option<String>,
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_chunk_deserialize_with_content() {
        let json = r#"{"choices":[{"delta":{"content":"Hello"}}]}"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_sse_chunk_deserialize_empty_delta() {
        // First SSE chunk often has role but no content
        let json = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices[0].delta.content.is_none());
    }

    #[test]
    fn test_sse_chunk_deserialize_empty_choices() {
        let json = r#"{"choices":[]}"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices.is_empty());
    }

    #[test]
    fn test_openai_llm_name() {
        let llm = OpenAiLlm::new("key".to_string(), "gpt-4o-mini".to_string());
        assert_eq!(llm.name(), "gpt-4o-mini");
    }

    #[test]
    fn test_openai_llm_name_custom_model() {
        let llm = OpenAiLlm::new("key".to_string(), "gpt-4o".to_string());
        assert_eq!(llm.name(), "gpt-4o");
    }
}
