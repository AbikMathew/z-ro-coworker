use crate::task_engine::types::{Task, TaskState};
use tauri::State;
use std::sync::{Arc, Mutex};
use crate::task_engine::machine::TaskMachine;

#[tauri::command]
pub fn list_tasks(machine: State<Arc<Mutex<TaskMachine>>>) -> Result<Vec<Task>, String> {
    let machine = machine.lock().map_err(|e| e.to_string())?;
    Ok(machine.list_tasks())
}

#[tauri::command]
pub fn start_task(task_id: String, machine: State<Arc<Mutex<TaskMachine>>>) -> Result<TaskState, String> {
    let mut machine = machine.lock().map_err(|e| e.to_string())?;
    machine.start_task(&task_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn advance_step(machine: State<Arc<Mutex<TaskMachine>>>) -> Result<TaskState, String> {
    let mut machine = machine.lock().map_err(|e| e.to_string())?;
    machine.advance_step().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_task_state(machine: State<Arc<Mutex<TaskMachine>>>) -> Result<Option<TaskState>, String> {
    let machine = machine.lock().map_err(|e| e.to_string())?;
    Ok(machine.get_state())
}
