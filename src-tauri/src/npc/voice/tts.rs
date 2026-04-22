use async_trait::async_trait;

/// The result of a TTS synthesis: complete audio bytes plus mime type.
///
/// We collect the full audio rather than streaming chunks to the frontend
/// because browser playback of streamed compressed audio requires
/// MediaSource Extensions, which is a significant complication for a
/// marginal latency win (TTS synth is typically 0.5–1.5 s).
#[derive(Debug, Clone)]
pub struct TtsAudio {
    /// MIME type of the audio (e.g. `"audio/mpeg"` for MP3).
    pub mime: String,
    /// Encoded audio bytes ready to be played by an HTML Audio element
    /// (as a data URL or Blob).
    pub bytes: Vec<u8>,
}

/// Text-to-Speech provider trait.
///
/// Implementations convert text into a complete audio buffer.  The
/// returned `mime` tells the frontend how to play the bytes (e.g. via
/// `new Audio("data:audio/mpeg;base64,...")`).
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Synthesize the given text.
    async fn synthesize(&self, text: &str) -> Result<TtsAudio, String>;

    /// Human-readable name for logging / status display.
    fn name(&self) -> &str;
}
