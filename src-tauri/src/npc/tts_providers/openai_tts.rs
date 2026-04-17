use async_trait::async_trait;

use crate::npc::voice::tts::{TtsAudio, TtsProvider};

const OPENAI_TTS_URL: &str = "https://api.openai.com/v1/audio/speech";

/// Fast / cheap default TTS model.
pub const DEFAULT_MODEL: &str = "tts-1";

/// Default voice — warm and casual, fits the "senior coworker" persona.
pub const DEFAULT_VOICE: &str = "onyx";

/// OpenAI TTS provider.
///
/// Returns MP3 bytes (`audio/mpeg`) which can be played directly in the
/// browser via `new Audio("data:audio/mpeg;base64,...")`.
pub struct OpenAiTts {
    client: reqwest::Client,
    api_key: String,
    model: String,
    voice: String,
}

impl OpenAiTts {
    pub fn new(api_key: String) -> Self {
        Self::with_options(api_key, DEFAULT_MODEL.to_string(), DEFAULT_VOICE.to_string())
    }

    pub fn with_options(api_key: String, model: String, voice: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
            voice,
        }
    }
}

#[async_trait]
impl TtsProvider for OpenAiTts {
    async fn synthesize(&self, text: &str) -> Result<TtsAudio, String> {
        let body = serde_json::json!({
            "model": self.model,
            "voice": self.voice,
            "input": text,
            "response_format": "mp3",
            "speed": 1.0,
        });

        let response = self
            .client
            .post(OPENAI_TTS_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI TTS request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(format!("OpenAI TTS error {}: {}", status, err_body));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("OpenAI TTS body read failed: {}", e))?
            .to_vec();

        Ok(TtsAudio {
            mime: "audio/mpeg".to_string(),
            bytes,
        })
    }

    fn name(&self) -> &str {
        &self.model
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model_voice() {
        let tts = OpenAiTts::new("key".to_string());
        assert_eq!(tts.name(), DEFAULT_MODEL);
        assert_eq!(tts.voice, DEFAULT_VOICE);
    }

    #[test]
    fn test_custom_model_voice() {
        let tts = OpenAiTts::with_options(
            "key".to_string(),
            "tts-1-hd".to_string(),
            "alloy".to_string(),
        );
        assert_eq!(tts.name(), "tts-1-hd");
        assert_eq!(tts.voice, "alloy");
    }
}
