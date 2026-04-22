use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub xp_reward: u32,
    pub steps: Vec<Step>,

    // ── Optional presentation fields (WS-C / dashboard) ────────────────
    //
    // These are all `#[serde(default)]` so existing task JSON files
    // without these fields continue to deserialize cleanly. The frontend
    // uses them to theme task cards and show at-a-glance metadata.
    /// Emoji or short glyph shown as the card thumbnail (e.g. "🎫").
    #[serde(default)]
    pub icon: Option<String>,
    /// One of `"coding" | "git" | "jira" | "communication" | "productivity"`.
    /// Drives the task card's gradient theme.
    #[serde(default)]
    pub category: Option<String>,
    /// `"beginner" | "intermediate" | "advanced"` — shown as a chip.
    #[serde(default)]
    pub difficulty: Option<String>,
    /// Approximate completion time in minutes.
    #[serde(default)]
    pub est_minutes: Option<u32>,
    /// Optional CSS color or gradient override (takes precedence over
    /// `category`-derived gradient).
    #[serde(default)]
    pub accent_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub instruction: String,
    pub hints: Vec<String>,
    pub validation: Option<ValidationRule>,
    pub overlay: Option<OverlayConfig>,
    pub xp: u32,

    /// Ordered sub-goals the user must satisfy to complete this step.
    /// Introduced in Phase 3c — older task JSON files omit this field and
    /// deserialize with `#[serde(default)]` to an empty vec, preserving
    /// the legacy "step advances on validation" behaviour.
    #[serde(default)]
    pub milestones: Vec<Milestone>,
}

/// A single checkpoint inside a `Step`. The Verifier evaluates each
/// milestone against the live screen context and decides whether the user
/// has satisfied it. Milestones are evaluated in order; the step advances
/// once every milestone is `Confirmed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    /// Stable identifier, e.g. `"m1"` or `"opened-terminal"`.
    pub id: String,
    /// Human-readable goal shown to the Actor LLM when describing what to
    /// do next. E.g. `"Terminal window is focused"`.
    pub goal_text: String,
    /// Condition the Verifier checks to decide whether this milestone is
    /// satisfied.
    pub verification: Predicate,
}

/// A condition the Verifier can evaluate over the current screen context.
///
/// The `kind` / `value` shape is internally tagged so a task JSON file
/// reads naturally:
///
/// ```json
/// {"kind": "window_title_contains", "value": "Terminal"}
/// ```
///
/// Non-LLM predicates are preferred — they're cheap, deterministic, and
/// don't burn tokens. `llm_judge` is reserved for cases where no simple
/// signal exists (e.g. "is the user looking at the right panel?").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Predicate {
    /// True when the focused window's title contains this substring
    /// (case-insensitive).
    WindowTitleContains(String),
    /// True when the focused app's process name equals this exactly
    /// (case-insensitive). Example: `"Terminal"`, `"Code"`.
    ProcessNameEquals(String),
    /// True when the indented AX tree text contains this substring
    /// (case-insensitive). Stand-in for "element visible" until we wire a
    /// proper AX walker that queries by role+label.
    AxTreeContains(String),
    /// Escape hatch: send the before/after screenshots + this question to a
    /// cheap LLM (Haiku class) and use its yes/no answer. Phase 3c ships
    /// the interface; the call is stubbed until the Actor role lands.
    LlmJudge(String),
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
