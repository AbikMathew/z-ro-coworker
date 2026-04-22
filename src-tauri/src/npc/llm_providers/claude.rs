use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::npc::prompt_builder::LlmMessage;
use crate::npc::voice::pipeline::LlmProvider;

/// Anthropic Claude chat-completion LLM provider with streaming support.
///
/// Supports the Messages API with SSE streaming
/// (`POST /v1/messages` + `stream: true`). Common models:
/// - `claude-haiku-4-5` — cheap, fast, capable vision.
/// - `claude-sonnet-4-5` — mid-tier reasoning + strong UI vision.
/// - `claude-opus-4-5` — premium; strongest reasoning & coord precision.
///
/// Shape deltas vs OpenAI:
/// - Auth: `x-api-key` header (no `Bearer`), plus `anthropic-version`.
/// - System prompt goes in a top-level `system` field, not as a role.
/// - Image blocks are `{"type":"image","source":{"type":"base64",...}}`.
/// - SSE events are named (`event: content_block_delta`) with `data: {...}`
///   per event; text deltas are `{"type":"text_delta","text":"..."}` inside
///   `content_block_delta` events.
pub struct ClaudeLlm {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl ClaudeLlm {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        }
    }
}

#[async_trait]
impl LlmProvider for ClaudeLlm {
    async fn stream_chat(
        &self,
        system: &str,
        messages: &[LlmMessage],
    ) -> Result<mpsc::Receiver<String>, String> {
        let body = build_request_body(&self.model, system, messages);

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Anthropic request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(format!("Anthropic API error {}: {}", status, body));
        }

        let (tx, rx) = mpsc::channel::<String>(64);
        let byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut stream = byte_stream;
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[z-ro:npc] Claude stream error: {}", e);
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // SSE: `event: <name>\ndata: {...}\n\n`. We only care about
                // `data:` lines — the event name is embedded in the JSON as
                // `"type"` too, so we can dispatch on that alone.
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            return;
                        }
                        if let Some(text) = extract_delta_text(data) {
                            if !text.is_empty() && tx.send(text).await.is_err() {
                                return; // receiver dropped
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

// ── Request body construction ──────────────────────────────────────────

/// Build the Anthropic `/v1/messages` request body from our internal
/// LlmMessage format.
///
/// Claude uses `"user"` / `"assistant"` roles (same as OpenAI). System prompt
/// sits in a top-level `system` field. Image blocks are distinct from text
/// blocks and carry `source.type=base64` with raw base64 (no data-URL prefix).
fn build_request_body(
    model: &str,
    system: &str,
    messages: &[LlmMessage],
) -> serde_json::Value {
    let api_messages: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            // Claude requires content to be an array of blocks when images
            // are present; for plain text we could pass a string, but the
            // array form is universally accepted, so use it everywhere to
            // keep the shape predictable.
            let mut blocks = Vec::with_capacity(m.images_b64.len() + 1);
            // Images first so the model sees them before the question —
            // matches the OpenAI convention and how we build the prompt.
            for b64 in &m.images_b64 {
                blocks.push(serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/jpeg",
                        "data": b64,
                    }
                }));
            }
            blocks.push(serde_json::json!({
                "type": "text",
                "text": m.content,
            }));
            serde_json::json!({
                "role": m.role,
                "content": blocks,
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": model,
        "system": system,
        "messages": api_messages,
        "max_tokens": 512,
        "stream": true,
    });
    // Opus 4.7 (and newer reasoning-class models) rejected `temperature`
    // in April 2026 — it's controlled via `thinking` params instead.
    // Older models (Haiku 4.5, Sonnet 4.5/4.6, Opus 4.5) still accept it
    // and behave better with an explicit 0.7 (Claude's default is 1.0
    // which is chattier than we want for short NPC replies).
    if accepts_temperature(model) {
        body["temperature"] = serde_json::json!(0.7);
    }
    body
}

/// Whether `temperature` is a valid request field for this model. As of
/// April 2026 the Opus 4.7+ line dropped it in favour of `thinking`.
/// Prefer an allow-list style check so a new unknown model doesn't get
/// silently blocked — only the explicitly-known "no-temperature" names
/// are stripped.
fn accepts_temperature(model: &str) -> bool {
    // Everything NOT in this list gets `temperature: 0.7`. Extend as
    // Anthropic deprecates the field on more models.
    const DEPRECATED_FOR: &[&str] = &["claude-opus-4-7"];
    !DEPRECATED_FOR.iter().any(|m| model == *m)
}

// ── SSE parsing ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SseEvent {
    #[serde(default, rename = "type")]
    event_type: Option<String>,
    #[serde(default)]
    delta: Option<SseDelta>,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    #[serde(default, rename = "type")]
    delta_type: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

/// Extract the text delta from a Claude SSE `data:` payload.
///
/// We only care about `content_block_delta` events carrying a `text_delta`.
/// Other event types (`message_start`, `content_block_start`, `ping`,
/// `message_delta`, `message_stop`) are ignored.
fn extract_delta_text(data: &str) -> Option<String> {
    let event: SseEvent = serde_json::from_str(data).ok()?;
    if event.event_type.as_deref() != Some("content_block_delta") {
        return None;
    }
    let delta = event.delta?;
    if delta.delta_type.as_deref() != Some("text_delta") {
        return None;
    }
    delta.text
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_llm_name() {
        let llm = ClaudeLlm::new("key".into(), "claude-haiku-4-5".into());
        assert_eq!(llm.name(), "claude-haiku-4-5");
    }

    #[test]
    fn test_claude_llm_name_sonnet() {
        let llm = ClaudeLlm::new("key".into(), "claude-sonnet-4-5".into());
        assert_eq!(llm.name(), "claude-sonnet-4-5");
    }

    // ── build_request_body ─────────────────────────────────────────────

    #[test]
    fn test_build_request_body_system_top_level() {
        let msgs = vec![LlmMessage {
            role: "user".into(),
            content: "hi".into(),
            images_b64: vec![],
        }];
        let body = build_request_body("claude-haiku-4-5", "SYS_XYZ", &msgs);
        // System prompt at the top level, NOT inside messages.
        assert_eq!(body["system"], "SYS_XYZ");
        for msg in body["messages"].as_array().unwrap() {
            assert_ne!(msg["role"], "system");
        }
    }

    #[test]
    fn test_build_request_body_role_passthrough() {
        // Claude uses "user" / "assistant" — same as our internal format.
        let msgs = vec![
            LlmMessage {
                role: "user".into(),
                content: "Q".into(),
                images_b64: vec![],
            },
            LlmMessage {
                role: "assistant".into(),
                content: "A".into(),
                images_b64: vec![],
            },
        ];
        let body = build_request_body("claude-haiku-4-5", "sys", &msgs);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][1]["role"], "assistant");
    }

    #[test]
    fn test_build_request_body_text_block_structure() {
        let msgs = vec![LlmMessage {
            role: "user".into(),
            content: "hello world".into(),
            images_b64: vec![],
        }];
        let body = build_request_body("claude-haiku-4-5", "sys", &msgs);
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        // Plain text = single text block in array form.
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "hello world");
    }

    #[test]
    fn test_build_request_body_with_image_block() {
        let msgs = vec![LlmMessage {
            role: "user".into(),
            content: "what is this?".into(),
            images_b64: vec!["ABC123".into()],
        }];
        let body = build_request_body("claude-haiku-4-5", "sys", &msgs);
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        // Image goes first, then text (so the model sees context before question).
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["type"], "base64");
        assert_eq!(blocks[0]["source"]["media_type"], "image/jpeg");
        assert_eq!(blocks[0]["source"]["data"], "ABC123");
        // Must be raw base64 — no data: URL wrapper.
        assert!(
            !blocks[0]["source"]["data"]
                .as_str()
                .unwrap()
                .starts_with("data:")
        );
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks[1]["text"], "what is this?");
    }

    #[test]
    fn test_build_request_body_multiple_images() {
        let msgs = vec![LlmMessage {
            role: "user".into(),
            content: "compare".into(),
            images_b64: vec!["A".into(), "B".into()],
        }];
        let body = build_request_body("claude-haiku-4-5", "sys", &msgs);
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 3); // 2 images + text
        assert_eq!(blocks[0]["source"]["data"], "A");
        assert_eq!(blocks[1]["source"]["data"], "B");
        assert_eq!(blocks[2]["type"], "text");
    }

    #[test]
    fn test_build_request_body_top_level_fields() {
        let msgs = vec![];
        let body = build_request_body("claude-haiku-4-5", "sys", &msgs);
        assert_eq!(body["model"], "claude-haiku-4-5");
        assert_eq!(body["max_tokens"], 512);
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn test_opus_4_7_omits_temperature() {
        // Opus 4.7 returns 400 Bad Request if `temperature` is present.
        let msgs = vec![];
        let body = build_request_body("claude-opus-4-7", "sys", &msgs);
        assert!(
            body.get("temperature").is_none(),
            "temperature should be absent for opus-4-7, got: {body}"
        );
        // Other fields still present.
        assert_eq!(body["model"], "claude-opus-4-7");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn test_haiku_still_carries_temperature() {
        let body = build_request_body("claude-haiku-4-5", "sys", &[]);
        assert_eq!(body["temperature"], 0.7);
    }

    #[test]
    fn test_accepts_temperature_allowlist_style() {
        assert!(accepts_temperature("claude-haiku-4-5"));
        assert!(accepts_temperature("claude-sonnet-4-6"));
        assert!(accepts_temperature("claude-opus-4-5"));
        assert!(!accepts_temperature("claude-opus-4-7"));
        // Unknown future model: err on the side of sending temperature
        // (the Anthropic API will reject if it's invalid, which surfaces
        // a clear error message).
        assert!(accepts_temperature("claude-future-5-0"));
    }

    // ── SSE parsing ────────────────────────────────────────────────────

    #[test]
    fn test_extract_delta_text_content_block_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        assert_eq!(extract_delta_text(data).as_deref(), Some("Hello"));
    }

    #[test]
    fn test_extract_delta_text_ignores_message_start() {
        let data = r#"{"type":"message_start","message":{"id":"msg_1","role":"assistant"}}"#;
        assert!(extract_delta_text(data).is_none());
    }

    #[test]
    fn test_extract_delta_text_ignores_content_block_start() {
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        assert!(extract_delta_text(data).is_none());
    }

    #[test]
    fn test_extract_delta_text_ignores_ping() {
        let data = r#"{"type":"ping"}"#;
        assert!(extract_delta_text(data).is_none());
    }

    #[test]
    fn test_extract_delta_text_ignores_message_delta() {
        // `message_delta` carries stop_reason/usage — not a text delta, even
        // though the envelope also has a `delta` field.
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":10}}"#;
        assert!(extract_delta_text(data).is_none());
    }

    #[test]
    fn test_extract_delta_text_ignores_message_stop() {
        let data = r#"{"type":"message_stop"}"#;
        assert!(extract_delta_text(data).is_none());
    }

    #[test]
    fn test_extract_delta_text_ignores_malformed_json() {
        assert!(extract_delta_text("not-json").is_none());
        assert!(extract_delta_text("").is_none());
    }
}
