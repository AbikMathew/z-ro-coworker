use super::types::ValidationRule;
use crate::context::WindowInfo;

pub fn validate_step(rule: &ValidationRule, window: &WindowInfo) -> bool {
    match rule.rule_type.as_str() {
        "app_context" => {
            if let Some(match_rule) = &rule.match_rule {
                if let Some(title_match) = &match_rule.window_title_contains {
                    return window.title.to_lowercase().contains(&title_match.to_lowercase());
                }
                if let Some(process_match) = &match_rule.process_name {
                    return window.process_name.to_lowercase().contains(&process_match.to_lowercase());
                }
            }
            false
        }
        _ => false,
    }
}
