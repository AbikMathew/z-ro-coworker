export interface Task {
  id: string;
  title: string;
  description: string;
  xp_reward: number;
  steps: Step[];
  // Optional presentation fields (WS-C dashboard)
  icon?: string | null;
  category?: TaskCategory | null;
  difficulty?: TaskDifficulty | null;
  est_minutes?: number | null;
  accent_color?: string | null;
}

export type TaskCategory =
  | "coding"
  | "git"
  | "jira"
  | "communication"
  | "productivity";

export type TaskDifficulty = "beginner" | "intermediate" | "advanced";

export interface Step {
  id: string;
  instruction: string;
  hints: string[];
  validation: ValidationRule | null;
  overlay: OverlayConfig | null;
  xp: number;
}

export interface ValidationRule {
  type: string;
  match: MatchRule | null;
  fallback_ai: boolean | null;
}

export interface MatchRule {
  window_title_contains: string | null;
  process_name: string | null;
}

export interface OverlayConfig {
  type: string;
  text: string | null;
  position: string | null;
}

export interface TaskState {
  task_id: string;
  current_step_index: number;
  current_step: Step;
  total_steps: number;
  status: "InProgress" | "Completed";
  xp_earned: number;
}
