//! Persisted user preferences that survive an app restart.
//!
//! Currently just the last-selected LLM (provider + model). Lives in a
//! JSON file under Tauri's platform-specific app data directory —
//! `~/Library/Application Support/com.zro.cowork/npc_settings.json` on
//! macOS. The file is plain JSON so it's trivially inspectable and
//! editable by hand when debugging.
//!
//! Why a bespoke file instead of Tauri's store plugin? We only have two
//! fields right now and the I/O is dead-simple; dragging in another
//! plugin for three values is overkill.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Persisted settings. Every field is optional so a missing / corrupt file
/// degrades gracefully to the first-boot defaults.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NpcSettings {
    pub llm_provider: Option<String>,
    pub llm_model: Option<String>,
}

impl NpcSettings {
    /// Load settings from `<app_data_dir>/npc_settings.json`. Returns
    /// `Default::default()` if the file is missing, unreadable, or
    /// malformed — we NEVER fail a boot over a bad settings file.
    pub fn load(app_data_dir: &Path) -> Self {
        let path = Self::path(app_data_dir);
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
                eprintln!(
                    "[z-ro:settings] {:?} is malformed — ignoring: {e}",
                    path
                );
                Self::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                eprintln!("[z-ro:settings] cannot read {:?}: {e}", path);
                Self::default()
            }
        }
    }

    /// Write the settings file. Creates the directory if needed. Errors
    /// are surfaced so the caller can log them (the app doesn't crash).
    pub fn save(&self, app_data_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(app_data_dir)
            .map_err(|e| format!("mkdir {:?}: {e}", app_data_dir))?;
        let path = Self::path(app_data_dir);
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| format!("serialize settings: {e}"))?;
        std::fs::write(&path, contents)
            .map_err(|e| format!("write {:?}: {e}", path))?;
        Ok(())
    }

    fn path(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join("npc_settings.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique test dir per run so concurrent `cargo test` threads don't
    /// fight over the same file. We use nanos + process id.
    fn isolated_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "zro-settings-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn load_from_missing_dir_returns_default() {
        let dir = isolated_dir("missing");
        let s = NpcSettings::load(&dir);
        assert!(s.llm_provider.is_none());
        assert!(s.llm_model.is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = isolated_dir("round-trip");
        let original = NpcSettings {
            llm_provider: Some("anthropic".into()),
            llm_model: Some("claude-haiku-4-5".into()),
        };
        original.save(&dir).unwrap();
        let loaded = NpcSettings::load(&dir);
        assert_eq!(loaded.llm_provider.as_deref(), Some("anthropic"));
        assert_eq!(loaded.llm_model.as_deref(), Some("claude-haiku-4-5"));
        // Cleanup — best-effort; the OS reaps /tmp either way.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_file_degrades_to_default_does_not_panic() {
        let dir = isolated_dir("malformed");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(NpcSettings::path(&dir), "{not json").unwrap();
        let s = NpcSettings::load(&dir);
        assert!(s.llm_provider.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
