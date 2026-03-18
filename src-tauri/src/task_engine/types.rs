use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub xp_reward: u32,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub instruction: String,
    pub hints: Vec<String>,
    pub validation: Option<ValidationRule>,
    pub overlay: Option<OverlayConfig>,
    pub xp: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    #[serde(rename = "type")]
    pub rule_type: String,
    #[serde(rename = "match")]
    pub match_rule: Option<MatchRule>,
    pub fallback_ai: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchRule {
    pub window_title_contains: Option<String>,
    pub process_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    #[serde(rename = "type")]
    pub overlay_type: String,
    pub text: Option<String>,
    pub position: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task_id: String,
    pub current_step_index: usize,
    pub current_step: Step,
    pub total_steps: usize,
    pub status: TaskStatus,
    pub xp_earned: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    InProgress,
    Completed,
}
