pub mod claude;
pub mod gemini;
pub mod openai;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::npc::voice::pipeline::LlmProvider;

// Each provider implements the LlmProvider trait from voice::pipeline.

// ── API key bag ────────────────────────────────────────────────────────

/// Holds all third-party API keys the NPC might need to swap between
/// providers at runtime.
#[derive(Debug, Clone, Default)]
pub struct ApiKeys {
    pub openai: String,
    pub gemini: String,
    pub anthropic: String,
    pub groq: String,
}

impl ApiKeys {
    pub fn from_env() -> Self {
        Self {
            openai: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            gemini: std::env::var("GEMINI_API_KEY").unwrap_or_default(),
            anthropic: std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            groq: std::env::var("GROQ_API_KEY").unwrap_or_default(),
        }
    }
}

// ── Model catalog ──────────────────────────────────────────────────────

/// Metadata for a single (provider, model) pair the user can choose from.
/// Only models whose API key is configured are surfaced to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// `"openai"` | `"gemini"` | `"anthropic"`
    pub provider: String,
    /// The API model id (e.g. `"gpt-4o-mini"`, `"gemini-2.0-flash"`).
    pub model: String,
    /// Human-friendly label for the UI.
    pub display_name: String,
    /// Whether this model accepts images.
    pub vision: bool,
    /// `"cheap"` | `"mid"` | `"premium"`.
    pub cost_tier: String,
    /// Short UX hint shown under the model name in settings.
    pub notes: String,
}

/// Return all models the user can pick from, filtered by which keys are set.
pub fn available_models(keys: &ApiKeys) -> Vec<ModelInfo> {
    let mut out = Vec::new();

    if !keys.openai.is_empty() {
        out.push(ModelInfo {
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            display_name: "GPT-4o mini".into(),
            vision: true,
            cost_tier: "cheap".into(),
            notes: "Fast & cheap. OK for text, weak on dense UIs.".into(),
        });
        out.push(ModelInfo {
            provider: "openai".into(),
            model: "gpt-4o".into(),
            display_name: "GPT-4o".into(),
            vision: true,
            cost_tier: "premium".into(),
            notes: "Best-in-class UI vision. Slower and pricey.".into(),
        });
    }

    if !keys.gemini.is_empty() {
        out.push(ModelInfo {
            provider: "gemini".into(),
            model: "gemini-2.0-flash".into(),
            display_name: "Gemini 2.0 Flash".into(),
            vision: true,
            cost_tier: "cheap".into(),
            notes: "Cheapest with solid vision grounding — good default.".into(),
        });
        out.push(ModelInfo {
            provider: "gemini".into(),
            model: "gemini-2.5-pro".into(),
            display_name: "Gemini 2.5 Pro".into(),
            vision: true,
            cost_tier: "mid".into(),
            notes: "Stronger reasoning & precise coordinates.".into(),
        });
    }

    if !keys.anthropic.is_empty() {
        out.push(ModelInfo {
            provider: "anthropic".into(),
            model: "claude-haiku-4-5".into(),
            display_name: "Claude Haiku 4.5".into(),
            vision: true,
            cost_tier: "cheap".into(),
            notes: "Fast & cheap Claude with solid vision — great fallback.".into(),
        });
        out.push(ModelInfo {
            provider: "anthropic".into(),
            model: "claude-sonnet-4-5".into(),
            display_name: "Claude Sonnet 4.5".into(),
            vision: true,
            cost_tier: "mid".into(),
            notes: "Balanced reasoning + strong UI vision.".into(),
        });
        out.push(ModelInfo {
            provider: "anthropic".into(),
            model: "claude-opus-4-5".into(),
            display_name: "Claude Opus 4.5".into(),
            vision: true,
            cost_tier: "premium".into(),
            notes: "Strongest reasoning & coordinate precision.".into(),
        });
    }

    out
}

/// Instantiate a provider instance from a (provider, model) pair and the
/// available API keys.
///
/// Returns `Arc<dyn LlmProvider>` so the pipeline can hot-swap the provider
/// behind an `RwLock` without copying the underlying state.
pub fn build_provider(
    provider: &str,
    model: &str,
    keys: &ApiKeys,
) -> Result<Arc<dyn LlmProvider>, String> {
    match provider {
        "openai" => {
            if keys.openai.is_empty() {
                return Err("OPENAI_API_KEY not configured".into());
            }
            Ok(Arc::new(openai::OpenAiLlm::new(
                keys.openai.clone(),
                model.to_string(),
            )))
        }
        "gemini" => {
            if keys.gemini.is_empty() {
                return Err("GEMINI_API_KEY not configured".into());
            }
            Ok(Arc::new(gemini::GeminiLlm::new(
                keys.gemini.clone(),
                model.to_string(),
            )))
        }
        "anthropic" => {
            if keys.anthropic.is_empty() {
                return Err("ANTHROPIC_API_KEY not configured".into());
            }
            Ok(Arc::new(claude::ClaudeLlm::new(
                keys.anthropic.clone(),
                model.to_string(),
            )))
        }
        other => Err(format!("Unknown LLM provider: {}", other)),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_models_empty_when_no_keys() {
        let keys = ApiKeys::default();
        let models = available_models(&keys);
        assert!(models.is_empty());
    }

    #[test]
    fn test_available_models_openai_only() {
        let keys = ApiKeys {
            openai: "sk-test".into(),
            ..Default::default()
        };
        let models = available_models(&keys);
        assert!(models.iter().all(|m| m.provider == "openai"));
        assert!(models.iter().any(|m| m.model == "gpt-4o-mini"));
        assert!(models.iter().any(|m| m.model == "gpt-4o"));
    }

    #[test]
    fn test_available_models_gemini_only() {
        let keys = ApiKeys {
            gemini: "AI-test".into(),
            ..Default::default()
        };
        let models = available_models(&keys);
        assert!(models.iter().all(|m| m.provider == "gemini"));
        assert!(models.iter().any(|m| m.model == "gemini-2.0-flash"));
    }

    #[test]
    fn test_available_models_anthropic_only() {
        let keys = ApiKeys {
            anthropic: "sk-ant-test".into(),
            ..Default::default()
        };
        let models = available_models(&keys);
        assert!(models.iter().all(|m| m.provider == "anthropic"));
        assert!(models.iter().any(|m| m.model == "claude-haiku-4-5"));
        assert!(models.iter().any(|m| m.model == "claude-sonnet-4-5"));
        assert!(models.iter().any(|m| m.model == "claude-opus-4-5"));
    }

    #[test]
    fn test_available_models_all_keys() {
        let keys = ApiKeys {
            openai: "sk".into(),
            gemini: "AI".into(),
            anthropic: "sk-ant".into(),
            groq: String::new(),
        };
        let models = available_models(&keys);
        // 2 OpenAI + 2 Gemini + 3 Claude
        assert_eq!(models.len(), 7);
    }

    // `Box<dyn LlmProvider>` isn't Debug, so these tests destructure by hand
    // rather than using `unwrap` / `unwrap_err`.

    #[test]
    fn test_build_provider_openai_without_key() {
        let keys = ApiKeys::default();
        match build_provider("openai", "gpt-4o-mini", &keys) {
            Err(e) => assert!(e.contains("OPENAI_API_KEY")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_build_provider_gemini_without_key() {
        let keys = ApiKeys::default();
        match build_provider("gemini", "gemini-2.0-flash", &keys) {
            Err(e) => assert!(e.contains("GEMINI_API_KEY")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_build_provider_anthropic_without_key() {
        let keys = ApiKeys::default();
        match build_provider("anthropic", "claude-haiku-4-5", &keys) {
            Err(e) => assert!(e.contains("ANTHROPIC_API_KEY")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_build_provider_unknown() {
        let keys = ApiKeys {
            openai: "x".into(),
            gemini: "y".into(),
            anthropic: "z".into(),
            groq: String::new(),
        };
        match build_provider("llama", "llama-3", &keys) {
            Err(e) => assert!(e.contains("Unknown")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_build_provider_openai_ok() {
        let keys = ApiKeys {
            openai: "sk-test".into(),
            ..Default::default()
        };
        match build_provider("openai", "gpt-4o-mini", &keys) {
            Ok(p) => assert_eq!(p.name(), "gpt-4o-mini"),
            Err(e) => panic!("expected Ok, got {}", e),
        }
    }

    #[test]
    fn test_build_provider_gemini_ok() {
        let keys = ApiKeys {
            gemini: "ai-test".into(),
            ..Default::default()
        };
        match build_provider("gemini", "gemini-2.0-flash", &keys) {
            Ok(p) => assert_eq!(p.name(), "gemini-2.0-flash"),
            Err(e) => panic!("expected Ok, got {}", e),
        }
    }

    #[test]
    fn test_build_provider_anthropic_ok() {
        let keys = ApiKeys {
            anthropic: "sk-ant-test".into(),
            ..Default::default()
        };
        match build_provider("anthropic", "claude-haiku-4-5", &keys) {
            Ok(p) => assert_eq!(p.name(), "claude-haiku-4-5"),
            Err(e) => panic!("expected Ok, got {}", e),
        }
    }
}
