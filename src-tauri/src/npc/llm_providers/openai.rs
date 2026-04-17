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
        let api_messages = build_api_messages(system, messages);

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

// ── Message → OpenAI JSON translation ──────────────────────────────────

/// Build the OpenAI `messages` array from our internal LlmMessage format.
///
/// - Plain text messages → `content: "..."` (string form)
/// - Messages with images → `content: [{type:"text",...},{type:"image_url",...}]`
///   using `data:image/jpeg;base64,<payload>` data URLs.
///
/// `detail: "high"` is used so the model can read small icon labels and UI
/// text — "low" caps at 85 tokens (≈ thumbnail) which loses the detail we
/// need when guiding a kid through a screen full of icons.
fn build_api_messages(system: &str, messages: &[LlmMessage]) -> Vec<serde_json::Value> {
    let mut api_messages = Vec::with_capacity(messages.len() + 1);
    api_messages.push(serde_json::json!({
        "role": "system",
        "content": system,
    }));
    for msg in messages {
        let content = if msg.images_b64.is_empty() {
            serde_json::Value::String(msg.content.clone())
        } else {
            let mut parts = Vec::with_capacity(msg.images_b64.len() + 1);
            parts.push(serde_json::json!({
                "type": "text",
                "text": msg.content,
            }));
            for b64 in &msg.images_b64 {
                parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/jpeg;base64,{}", b64),
                        "detail": "high",
                    }
                }));
            }
            serde_json::Value::Array(parts)
        };
        api_messages.push(serde_json::json!({
            "role": msg.role,
            "content": content,
        }));
    }
    api_messages
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

    // ── build_api_messages ─────────────────────────────────────────────

    #[test]
    fn test_build_api_messages_plain_text() {
        let msgs = vec![LlmMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
            images_b64: vec![],
        }];
        let result = build_api_messages("sys", &msgs);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["role"], "system");
        assert_eq!(result[0]["content"], "sys");
        assert_eq!(result[1]["role"], "user");
        // Plain text → string content
        assert_eq!(result[1]["content"], "hello");
        assert!(result[1]["content"].is_string());
    }

    #[test]
    fn test_build_api_messages_with_image() {
        let msgs = vec![LlmMessage {
            role: "user".to_string(),
            content: "what is this?".to_string(),
            images_b64: vec!["ABC123".to_string()],
        }];
        let result = build_api_messages("sys", &msgs);
        let content = &result[1]["content"];
        // Multimodal → array content
        assert!(content.is_array());
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 2); // text + image
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "what is this?");
        assert_eq!(arr[1]["type"], "image_url");
        assert_eq!(
            arr[1]["image_url"]["url"],
            "data:image/jpeg;base64,ABC123"
        );
        assert_eq!(arr[1]["image_url"]["detail"], "high");
    }

    #[test]
    fn test_build_api_messages_mixed_history() {
        // History messages (no images) + current (with image) should produce
        // string-form content for history and array-form for current.
        let msgs = vec![
            LlmMessage {
                role: "user".to_string(),
                content: "earlier".to_string(),
                images_b64: vec![],
            },
            LlmMessage {
                role: "assistant".to_string(),
                content: "ok".to_string(),
                images_b64: vec![],
            },
            LlmMessage {
                role: "user".to_string(),
                content: "now".to_string(),
                images_b64: vec!["IMG".to_string()],
            },
        ];
        let result = build_api_messages("sys", &msgs);
        assert_eq!(result.len(), 4); // system + 3
        assert!(result[1]["content"].is_string());
        assert!(result[2]["content"].is_string());
        assert!(result[3]["content"].is_array());
    }

    #[test]
    fn test_build_api_messages_multiple_images() {
        let msgs = vec![LlmMessage {
            role: "user".to_string(),
            content: "comparing".to_string(),
            images_b64: vec!["A".to_string(), "B".to_string()],
        }];
        let result = build_api_messages("sys", &msgs);
        let arr = result[1]["content"].as_array().unwrap();
        assert_eq!(arr.len(), 3); // text + 2 images
        assert_eq!(arr[1]["image_url"]["url"], "data:image/jpeg;base64,A");
        assert_eq!(arr[2]["image_url"]["url"], "data:image/jpeg;base64,B");
    }
}
