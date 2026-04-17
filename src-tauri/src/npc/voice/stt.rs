use async_trait::async_trait;

/// Speech-to-Text provider trait.
///
/// Implementations convert audio bytes into text.  The `filename` argument
/// is a format hint (e.g. `"audio.webm"`, `"audio.mp3"`, `"audio.wav"`) —
/// providers like Groq / OpenAI use the extension to detect the codec.
/// MediaRecorder in browsers produces WebM/Opus by default, which is
/// natively supported by both Whisper-based endpoints.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Transcribe the given audio bytes to text.
    async fn transcribe(&self, audio_bytes: &[u8], filename: &str) -> Result<String, String>;

    /// Human-readable name for logging / status display.
    fn name(&self) -> &str;
}
