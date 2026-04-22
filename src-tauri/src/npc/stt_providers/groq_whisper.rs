use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;

use crate::npc::voice::stt::SttProvider;

/// Groq Whisper endpoint — OpenAI-compatible transcription API.
const GROQ_TRANSCRIPTIONS_URL: &str =
    "https://api.groq.com/openai/v1/audio/transcriptions";

/// Fastest Whisper variant on Groq, optimized for latency.
pub const DEFAULT_MODEL: &str = "whisper-large-v3-turbo";

/// Groq Whisper-based STT provider.
///
/// Uses Groq's OpenAI-compatible transcriptions endpoint, which accepts
/// common audio formats (webm, mp3, m4a, wav, ogg, flac) up to 25 MB.
/// Returns JSON by default; we parse just the `text` field.
pub struct GroqWhisperStt {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl GroqWhisperStt {
    pub fn new(api_key: String) -> Self {
        Self::with_model(api_key, DEFAULT_MODEL.to_string())
    }

    pub fn with_model(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        }
    }
}

#[async_trait]
impl SttProvider for GroqWhisperStt {
    async fn transcribe(&self, audio_bytes: &[u8], filename: &str) -> Result<String, String> {
        let mime = mime_for_filename(filename);

        let file_part = multipart::Part::bytes(audio_bytes.to_vec())
            .file_name(filename.to_string())
            .mime_str(mime)
            .map_err(|e| format!("Bad mime for {}: {}", filename, e))?;

        let form = multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("response_format", "json")
            .text("temperature", "0");

        let response = self
            .client
            .post(GROQ_TRANSCRIPTIONS_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Groq request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(format!("Groq STT error {}: {}", status, body));
        }

        let parsed: TranscriptionResponse = response
            .json()
            .await
            .map_err(|e| format!("Groq response parse failed: {}", e))?;

        Ok(parsed.text.trim().to_string())
    }

    fn name(&self) -> &str {
        &self.model
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Best-effort MIME type from file extension.  Groq / Whisper actually
/// infers format from the extension of the uploaded filename, but
/// `multipart::Part::mime_str` also requires a valid MIME.
pub(crate) fn mime_for_filename(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();
    if lower.ends_with(".webm") {
        "audio/webm"
    } else if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".wav") {
        "audio/wav"
    } else if lower.ends_with(".m4a") {
        "audio/mp4"
    } else if lower.ends_with(".ogg") || lower.ends_with(".opus") {
        "audio/ogg"
    } else if lower.ends_with(".flac") {
        "audio/flac"
    } else {
        "application/octet-stream"
    }
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model() {
        let stt = GroqWhisperStt::new("key".to_string());
        assert_eq!(stt.name(), DEFAULT_MODEL);
    }

    #[test]
    fn test_custom_model() {
        let stt = GroqWhisperStt::with_model("key".to_string(), "whisper-large-v3".to_string());
        assert_eq!(stt.name(), "whisper-large-v3");
    }

    #[test]
    fn test_mime_webm() {
        assert_eq!(mime_for_filename("audio.webm"), "audio/webm");
        assert_eq!(mime_for_filename("AUDIO.WEBM"), "audio/webm");
    }

    #[test]
    fn test_mime_mp3_wav_m4a() {
        assert_eq!(mime_for_filename("a.mp3"), "audio/mpeg");
        assert_eq!(mime_for_filename("a.wav"), "audio/wav");
        assert_eq!(mime_for_filename("a.m4a"), "audio/mp4");
    }

    #[test]
    fn test_mime_ogg_opus() {
        assert_eq!(mime_for_filename("a.ogg"), "audio/ogg");
        assert_eq!(mime_for_filename("a.opus"), "audio/ogg");
    }

    #[test]
    fn test_mime_unknown_falls_back() {
        assert_eq!(mime_for_filename("a.xyz"), "application/octet-stream");
        assert_eq!(mime_for_filename("no_extension"), "application/octet-stream");
    }

    #[test]
    fn test_transcription_response_deserialize() {
        let json = r#"{"text":"Hello world.","x_groq":{"id":"abc"}}"#;
        let parsed: TranscriptionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.text, "Hello world.");
    }
}
