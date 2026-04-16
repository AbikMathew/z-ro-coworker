use async_trait::async_trait;

/// Speech-to-Text provider trait.
///
/// Implementations convert raw PCM audio (16 kHz, mono, i16 LE) into text.
/// Phase 1 leaves this as a trait definition only — no provider is wired yet
/// because the MVP uses text-only input.  Phase 2 adds Groq Whisper.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Transcribe raw PCM audio (16 kHz, mono, i16 little-endian) to text.
    async fn transcribe(&self, pcm_16k_mono: &[u8]) -> Result<String, String>;

    /// Human-readable name for logging / status display.
    fn name(&self) -> &str;
}
