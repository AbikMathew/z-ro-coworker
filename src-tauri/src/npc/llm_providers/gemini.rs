use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::npc::prompt_builder::LlmMessage;
use crate::npc::voice::pipeline::LlmProvider;

/// Google Gemini chat-completion LLM provider with streaming support.
///
/// Supports `gemini-2.0-flash` (fast, cheap, strong vision) and
/// `gemini-2.5-pro` (more expensive, stronger reasoning / precise coords).
///
/// Uses Server-Sent Events (SSE) streaming via the `:streamGenerateContent`
/// endpoint with `?alt=sse` so each chunk is a full JSON object with
/// `candidates[0].content.parts[0].text` deltas.
pub struct GeminiLlm {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl GeminiLlm {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiLlm {
    async fn stream_chat(
        &self,
        system: &str,
        messages: &[LlmMessage],
    ) -> Result<mpsc::Receiver<String>, String> {
        let body = build_request_body(system, messages);

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.model, self.api_key,
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(format!("Gemini API error {}: {}", status, body));
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
                        eprintln!("[z-ro:npc] Gemini stream error: {}", e);
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // SSE: newline-separated records, `data: {...}` per line.
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        // Gemini doesn't emit a [DONE] sentinel, but guard anyway.
                        if data == "[DONE]" {
                            return;
                        }
                        if let Ok(parsed) = serde_json::from_str::<SseChunk>(data) {
                            if let Some(text) = extract_text(&parsed) {
                                if !text.is_empty() && tx.send(text).await.is_err() {
                                    return; // Receiver dropped
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

// ── Request body construction ──────────────────────────────────────────

/// Build the Gemini `:streamGenerateContent` request body from our internal
/// LlmMessage format.
///
/// Shape differences vs OpenAI:
/// - System prompt is a top-level `system_instruction`, not a role.
/// - Our `"assistant"` role maps to Gemini's `"model"` role.
/// - Images are `inline_data` with `mime_type` + raw base64 (no data URL).
fn build_request_body(system: &str, messages: &[LlmMessage]) -> serde_json::Value {
    let contents: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let role = if m.role == "assistant" { "model" } else { "user" };
            let mut parts = Vec::with_capacity(m.images_b64.len() + 1);
            parts.push(serde_json::json!({ "text": m.content }));
            for b64 in &m.images_b64 {
                parts.push(serde_json::json!({
                    "inline_data": {
                        "mime_type": "image/jpeg",
                        "data": b64,
                    }
                }));
            }
            serde_json::json!({ "role": role, "parts": parts })
        })
        .collect();

    serde_json::json!({
        "system_instruction": { "parts": [{ "text": system }] },
        "contents": contents,
        "generationConfig": {
            "temperature": 0.7,
            "maxOutputTokens": 512,
        }
    })
}

// ── SSE JSON types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SseChunk {
    #[serde(default)]
    candidates: Vec<SseCandidate>,
}

#[derive(Debug, Deserialize)]
struct SseCandidate {
    #[serde(default)]
    content: Option<SseContent>,
}

#[derive(Debug, Deserialize)]
struct SseContent {
    #[serde(default)]
    parts: Vec<SsePart>,
}

#[derive(Debug, Deserialize)]
struct SsePart {
    #[serde(default)]
    text: Option<String>,
}

/// Join all text parts of the first candidate (usually only one part per chunk,
/// but we concatenate defensively).
fn extract_text(chunk: &SseChunk) -> Option<String> {
    let first = chunk.candidates.first()?;
    let content = first.content.as_ref()?;
    let text: String = content
        .parts
        .iter()
        .filter_map(|p| p.text.as_deref())
        .collect();
    Some(text)
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_llm_name() {
        let llm = GeminiLlm::new("key".to_string(), "gemini-2.0-flash".to_string());
        assert_eq!(llm.name(), "gemini-2.0-flash");
    }

    #[test]
    fn test_gemini_llm_name_pro() {
        let llm = GeminiLlm::new("key".to_string(), "gemini-2.5-pro".to_string());
        assert_eq!(llm.name(), "gemini-2.5-pro");
    }

    // ── build_request_body ─────────────────────────────────────────────

    #[test]
    fn test_build_request_body_system_instruction_separated() {
        let msgs = vec![LlmMessage {
            role: "user".into(),
            content: "hello".into(),
            images_b64: vec![],
        }];
        let body = build_request_body("SYS_PROMPT_XYZ", &msgs);
        // System prompt must go in system_instruction, NOT the contents array
        assert_eq!(
            body["system_instruction"]["parts"][0]["text"],
            "SYS_PROMPT_XYZ"
        );
        assert!(body["contents"].is_array());
        for msg in body["contents"].as_array().unwrap() {
            assert_ne!(msg["role"], "system");
        }
    }

    #[test]
    fn test_build_request_body_role_mapping() {
        // "assistant" must be mapped to Gemini's "model" role.
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
        let body = build_request_body("sys", &msgs);
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][1]["role"], "model");
    }

    #[test]
    fn test_build_request_body_text_part_structure() {
        let msgs = vec![LlmMessage {
            role: "user".into(),
            content: "hi there".into(),
            images_b64: vec![],
        }];
        let body = build_request_body("sys", &msgs);
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hi there");
    }

    #[test]
    fn test_build_request_body_with_image_inline_data() {
        let msgs = vec![LlmMessage {
            role: "user".into(),
            content: "look".into(),
            images_b64: vec!["ABC123".into()],
        }];
        let body = build_request_body("sys", &msgs);
        let parts = body["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2); // text + image
        // Image part must use inline_data with mime_type + raw base64
        assert_eq!(parts[1]["inline_data"]["mime_type"], "image/jpeg");
        assert_eq!(parts[1]["inline_data"]["data"], "ABC123");
        // Gemini does NOT take a "data:" URL prefix
        assert!(
            !parts[1]["inline_data"]["data"]
                .as_str()
                .unwrap()
                .starts_with("data:")
        );
    }

    #[test]
    fn test_build_request_body_multiple_images() {
        let msgs = vec![LlmMessage {
            role: "user".into(),
            content: "compare".into(),
            images_b64: vec!["A".into(), "B".into()],
        }];
        let body = build_request_body("sys", &msgs);
        let parts = body["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 3); // text + 2 images
        assert_eq!(parts[1]["inline_data"]["data"], "A");
        assert_eq!(parts[2]["inline_data"]["data"], "B");
    }

    #[test]
    fn test_build_request_body_generation_config() {
        let msgs = vec![];
        let body = build_request_body("sys", &msgs);
        assert_eq!(body["generationConfig"]["temperature"], 0.7);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 512);
    }

    // ── SSE parsing ────────────────────────────────────────────────────

    #[test]
    fn test_sse_chunk_deserialize_with_text() {
        let json = r#"{"candidates":[{"content":{"parts":[{"text":"Hello"}],"role":"model"}}]}"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&chunk).as_deref(), Some("Hello"));
    }

    #[test]
    fn test_sse_chunk_deserialize_empty_candidates() {
        let json = r#"{"candidates":[]}"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert!(extract_text(&chunk).is_none());
    }

    #[test]
    fn test_sse_chunk_deserialize_missing_text_part() {
        let json = r#"{"candidates":[{"content":{"parts":[{}]}}]}"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        // extract_text returns Some("") — not an error
        assert_eq!(extract_text(&chunk).as_deref(), Some(""));
    }

    #[test]
    fn test_sse_chunk_deserialize_ignores_unknown_fields() {
        // Real Gemini responses include usageMetadata, finishReason, etc.
        let json = r#"{"candidates":[{"content":{"parts":[{"text":"hi"}]},"finishReason":"STOP"}],"usageMetadata":{"totalTokens":5}}"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&chunk).as_deref(), Some("hi"));
    }

    #[test]
    fn test_sse_chunk_multiple_parts_concatenated() {
        let json = r#"{"candidates":[{"content":{"parts":[{"text":"foo"},{"text":"bar"}]}}]}"#;
        let chunk: SseChunk = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&chunk).as_deref(), Some("foobar"));
    }
}
