use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProgress {
    pub total_xp: u32,
    pub tasks_completed: u32,
    pub current_streak: u32,
    pub skills: Vec<SkillProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillProgress {
    pub name: String,
    pub level: u32,
    pub xp: u32,
}

impl Default for UserProgress {
    fn default() -> Self {
        Self {
            total_xp: 0,
            tasks_completed: 0,
            current_streak: 0,
            skills: vec![],
        }
    }
}
