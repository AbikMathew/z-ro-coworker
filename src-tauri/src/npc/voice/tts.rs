use async_trait::async_trait;
use tokio::sync::mpsc;

/// Text-to-Speech provider trait.
///
/// Implementations convert text into audio chunks streamed through an mpsc
/// channel for low-latency playback.  Phase 1 is text-only so no provider is
/// wired yet — Phase 2 adds OpenAI TTS.
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Convert text to PCM audio (24 kHz, mono, i16 LE).
    ///
    /// Returns a receiver that yields audio chunks as they become available.
    /// The channel is closed when synthesis is complete.
    async fn synthesize(&self, text: &str) -> Result<mpsc::Receiver<Vec<u8>>, String>;

    /// Human-readable name for logging / status display.
    fn name(&self) -> &str;
}
