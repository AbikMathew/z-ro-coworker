pub mod coordinator;
pub mod screen_reader;
pub mod prompt_builder;
pub mod conversation;
pub mod voice;
pub mod llm_providers;
pub mod stt_providers;
pub mod tts_providers;
pub mod overlay_driver;
pub mod frame_buffer;
pub mod interrupt;
pub mod verifier;
pub mod settings_store;
#[cfg(target_os = "macos")]
pub mod events;

pub use coordinator::NpcCoordinator;
