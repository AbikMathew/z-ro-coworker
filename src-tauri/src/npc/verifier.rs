//! Evaluates `Milestone` predicates against the live screen context.
//!
//! Phase 3c ships the primitive in isolation — the reactive coordinator
//! loop that drives it on every OS event lands in Phase 3b. For now the
//! verifier is callable on demand:
//!
//! ```ignore
//! let v = Verifier::new(frame_buffer);
//! let result = v.evaluate(&milestone, &screen);
//! match result {
//!     VerifyResult::Confirmed => advance,
//!     VerifyResult::NotYet    => keep guiding,
//!     VerifyResult::Undecided => escalate to LLM,
//! }
//! ```
//!
//! The non-LLM predicates (`WindowTitleContains`, `ProcessNameEquals`,
//! `AxTreeContains`) are deterministic and free. `LlmJudge` is wired as
//! `VerifyResult::Undecided` until the Actor role lands — we return
//! "needs a model" rather than guessing, so callers can decide when to
//! pay the token cost.

use crate::npc::frame_buffer::FrameBuffer;
use crate::npc::screen_reader::ScreenContext;
use crate::task_engine::types::{Milestone, Predicate};

/// Outcome of evaluating a milestone against a snapshot of the screen.
#[derive(Debug, Clone, PartialEq)]
pub enum VerifyResult {
    /// Predicate fired — the user has satisfied this milestone.
    Confirmed,
    /// Predicate did not match. The user hasn't done it yet (or at all).
    NotYet,
    /// This predicate requires an LLM judge and the verifier isn't wired
    /// to one. Caller should escalate.
    Undecided,
}

/// Stateless helper (for now) that evaluates predicates. Holds a handle
/// to the `FrameBuffer` so future `LlmJudge` predicates can pull the
/// before/after frames without a separate parameter.
pub struct Verifier {
    #[allow(dead_code)] // used once LlmJudge is wired in Phase 3c integration
    frame_buffer: FrameBuffer,
}

impl Verifier {
    pub fn new(frame_buffer: FrameBuffer) -> Self {
        Self { frame_buffer }
    }

    /// Evaluate `milestone` against the current screen context. Returns
    /// `Confirmed`, `NotYet`, or `Undecided`.
    pub fn evaluate(&self, milestone: &Milestone, screen: &ScreenContext) -> VerifyResult {
        match &milestone.verification {
            Predicate::WindowTitleContains(needle) => {
                let hay = screen.window.title.to_lowercase();
                let needle = needle.to_lowercase();
                if hay.contains(&needle) {
                    VerifyResult::Confirmed
                } else {
                    VerifyResult::NotYet
                }
            }
            Predicate::ProcessNameEquals(expected) => {
                if screen.window.process_name.eq_ignore_ascii_case(expected) {
                    VerifyResult::Confirmed
                } else {
                    VerifyResult::NotYet
                }
            }
            Predicate::AxTreeContains(needle) => {
                let Some(tree) = screen.ax_tree.as_deref() else {
                    // No AX tree means we can't evaluate — caller may
                    // want to escalate to a vision model.
                    return VerifyResult::Undecided;
                };
                if tree.to_lowercase().contains(&needle.to_lowercase()) {
                    VerifyResult::Confirmed
                } else {
                    VerifyResult::NotYet
                }
            }
            Predicate::LlmJudge(_) => VerifyResult::Undecided,
        }
    }

    /// Run every milestone in `milestones` against `screen` and return
    /// the index of the first that is NOT `Confirmed`. Returns `None`
    /// when all milestones are satisfied — the step is done.
    ///
    /// This is the shape the reactive loop will use: "where are we in
    /// the milestone list, should we advance?"
    pub fn first_unsatisfied<'a>(
        &self,
        milestones: &'a [Milestone],
        screen: &ScreenContext,
    ) -> Option<(usize, &'a Milestone, VerifyResult)> {
        for (i, m) in milestones.iter().enumerate() {
            let r = self.evaluate(m, screen);
            if r != VerifyResult::Confirmed {
                return Some((i, m, r));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::WindowInfo;

    fn screen_with(title: &str, process: &str, ax: Option<&str>) -> ScreenContext {
        ScreenContext {
            window: WindowInfo {
                title: title.to_string(),
                process_name: process.to_string(),
                bundle_id: None,
                pid: None,
            },
            ax_tree: ax.map(String::from),
            screenshot_b64: None,
            captured_at_ms: 0,
        }
    }

    fn verifier() -> Verifier {
        Verifier::new(FrameBuffer::new())
    }

    fn milestone(id: &str, verification: Predicate) -> Milestone {
        Milestone {
            id: id.to_string(),
            goal_text: format!("goal for {id}"),
            verification,
        }
    }

    #[test]
    fn window_title_match_is_case_insensitive() {
        let v = verifier();
        let s = screen_with("MY Terminal", "Terminal", None);
        let m = milestone("m1", Predicate::WindowTitleContains("terminal".into()));
        assert_eq!(v.evaluate(&m, &s), VerifyResult::Confirmed);
    }

    #[test]
    fn window_title_no_match_returns_not_yet() {
        let v = verifier();
        let s = screen_with("Finder", "Finder", None);
        let m = milestone("m1", Predicate::WindowTitleContains("Terminal".into()));
        assert_eq!(v.evaluate(&m, &s), VerifyResult::NotYet);
    }

    #[test]
    fn process_name_equals_is_case_insensitive() {
        let v = verifier();
        let s = screen_with("main.rs", "Code", None);
        let m = milestone("m1", Predicate::ProcessNameEquals("code".into()));
        assert_eq!(v.evaluate(&m, &s), VerifyResult::Confirmed);
    }

    #[test]
    fn ax_tree_contains_empty_tree_is_undecided() {
        let v = verifier();
        let s = screen_with("Whatever", "SomeApp", None);
        let m = milestone("m1", Predicate::AxTreeContains("Submit".into()));
        assert_eq!(v.evaluate(&m, &s), VerifyResult::Undecided);
    }

    #[test]
    fn ax_tree_contains_matches() {
        let v = verifier();
        let ax = "window \"Dialog\"\n  button \"Submit\"\n  textfield \"Email\"";
        let s = screen_with("Dialog", "App", Some(ax));
        let m = milestone("m1", Predicate::AxTreeContains("submit".into()));
        assert_eq!(v.evaluate(&m, &s), VerifyResult::Confirmed);
    }

    #[test]
    fn llm_judge_is_undecided_until_wired() {
        let v = verifier();
        let s = screen_with("Any", "Any", None);
        let m = milestone("m1", Predicate::LlmJudge("Is the user on the right tab?".into()));
        assert_eq!(v.evaluate(&m, &s), VerifyResult::Undecided);
    }

    #[test]
    fn first_unsatisfied_stops_at_first_not_yet() {
        let v = verifier();
        let s = screen_with("Terminal", "Terminal", Some("window \"Terminal\""));
        let milestones = vec![
            milestone("m1", Predicate::ProcessNameEquals("Terminal".into())),
            milestone("m2", Predicate::AxTreeContains("git clone".into())),
            milestone("m3", Predicate::AxTreeContains("Cloning into".into())),
        ];
        let first = v.first_unsatisfied(&milestones, &s);
        assert!(first.is_some());
        let (idx, m, result) = first.unwrap();
        assert_eq!(idx, 1);
        assert_eq!(m.id, "m2");
        assert_eq!(result, VerifyResult::NotYet);
    }

    #[test]
    fn first_unsatisfied_all_confirmed_returns_none() {
        let v = verifier();
        let s = screen_with("Terminal — git", "Terminal", None);
        let milestones = vec![
            milestone("m1", Predicate::ProcessNameEquals("Terminal".into())),
            milestone("m2", Predicate::WindowTitleContains("git".into())),
        ];
        assert!(v.first_unsatisfied(&milestones, &s).is_none());
    }

    #[test]
    fn predicate_serde_round_trip() {
        // Round-trips what a task JSON file would look like.
        let p = Predicate::WindowTitleContains("Chrome".into());
        let s = serde_json::to_string(&p).unwrap();
        assert!(s.contains("window_title_contains"), "got {s}");
        assert!(s.contains("Chrome"));
        let back: Predicate = serde_json::from_str(&s).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn task_without_milestones_still_deserializes() {
        // Legacy task JSON files predate Phase 3c and don't include a
        // `milestones` field at all. They must continue to parse.
        use crate::task_engine::types::Step;
        let json = r#"{
            "id": "legacy",
            "instruction": "Do the thing",
            "hints": [],
            "validation": null,
            "overlay": null,
            "xp": 10
        }"#;
        let step: Step = serde_json::from_str(json).unwrap();
        assert!(step.milestones.is_empty());
    }

    #[test]
    fn task_with_milestones_deserializes() {
        use crate::task_engine::types::Step;
        let json = r#"{
            "id": "new",
            "instruction": "Open Terminal",
            "hints": [],
            "validation": null,
            "overlay": null,
            "xp": 10,
            "milestones": [
                {
                    "id": "m1",
                    "goal_text": "Terminal is focused",
                    "verification": {"kind": "process_name_equals", "value": "Terminal"}
                }
            ]
        }"#;
        let step: Step = serde_json::from_str(json).unwrap();
        assert_eq!(step.milestones.len(), 1);
        assert_eq!(step.milestones[0].id, "m1");
        matches!(
            step.milestones[0].verification,
            Predicate::ProcessNameEquals(_)
        );
    }

    #[test]
    fn every_shipped_task_json_parses() {
        // Walk the tasks/ directory and parse every file. Phase 3c added
        // milestones to one task — this guards against a careless edit
        // that breaks the loader for the rest.
        use crate::task_engine::types::Task;
        use std::fs;
        use std::path::PathBuf;

        let tasks_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tasks");
        assert!(
            tasks_dir.is_dir(),
            "tasks/ not found at {:?}",
            tasks_dir
        );

        let entries = fs::read_dir(&tasks_dir).unwrap();
        let mut count = 0;
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let content = fs::read_to_string(&path).unwrap();
            let task: Task = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("failed to parse {:?}: {e}", path));
            assert!(!task.id.is_empty(), "task at {:?} has empty id", path);
            assert!(
                !task.steps.is_empty(),
                "task at {:?} has no steps",
                path
            );
            count += 1;
        }
        assert!(count >= 1, "no task JSON files found");
    }
}
