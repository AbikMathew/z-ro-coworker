use super::types::{Task, TaskState, TaskStatus};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct TaskMachine {
    tasks: HashMap<String, Task>,
    current_task: Option<Task>,
    current_step_index: usize,
    xp_earned: u32,
}

impl TaskMachine {
    pub fn new(tasks_dir: PathBuf) -> Self {
        let mut tasks = HashMap::new();

        if let Ok(entries) = fs::read_dir(&tasks_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map_or(false, |ext| ext == "json") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(task) = serde_json::from_str::<Task>(&content) {
                            tasks.insert(task.id.clone(), task);
                        }
                    }
                }
            }
        }

        Self {
            tasks,
            current_task: None,
            current_step_index: 0,
            xp_earned: 0,
        }
    }

    pub fn start_task(&mut self, task_id: &str) -> Result<TaskState, String> {
        let task = self.tasks.get(task_id)
            .ok_or_else(|| format!("Task '{}' not found", task_id))?
            .clone();

        self.current_step_index = 0;
        self.xp_earned = 0;
        let state = self.build_state(&task);
        self.current_task = Some(task);
        Ok(state)
    }

    pub fn advance_step(&mut self) -> Result<TaskState, String> {
        let task = self.current_task.as_ref()
            .ok_or("No active task")?
            .clone();

        // Award XP for completed step
        self.xp_earned += task.steps[self.current_step_index].xp;
        self.current_step_index += 1;

        if self.current_step_index >= task.steps.len() {
            // Task complete — award bonus XP
            self.xp_earned += task.xp_reward;
            let mut state = self.build_state(&task);
            state.current_step_index = task.steps.len() - 1;
            state.current_step = task.steps.last().unwrap().clone();
            state.status = TaskStatus::Completed;
            self.current_task = None;
            return Ok(state);
        }

        Ok(self.build_state(&task))
    }

    pub fn get_state(&self) -> Option<TaskState> {
        self.current_task.as_ref().map(|task| self.build_state(task))
    }

    pub fn list_tasks(&self) -> Vec<Task> {
        self.tasks.values().cloned().collect()
    }

    fn build_state(&self, task: &Task) -> TaskState {
        let step_index = self.current_step_index.min(task.steps.len() - 1);
        TaskState {
            task_id: task.id.clone(),
            current_step_index: step_index,
            current_step: task.steps[step_index].clone(),
            total_steps: task.steps.len(),
            status: TaskStatus::InProgress,
            xp_earned: self.xp_earned,
        }
    }
}
