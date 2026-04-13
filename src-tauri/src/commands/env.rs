/// Read the Gemini API key from the process environment.
///
/// The key is loaded from `.env` at startup by `dotenvy` in `lib.rs::run()`.
/// This command exists so the frontend can obtain the key at runtime without
/// embedding it into the compiled JS bundle. In production, swap this for an
/// ephemeral-token endpoint.
#[tauri::command]
pub fn get_gemini_api_key() -> Result<String, String> {
    match std::env::var("GEMINI_API_KEY") {
        Ok(key) if !key.is_empty() => Ok(key),
        _ => Err("GEMINI_API_KEY not set. Add it to .env in the project root.".to_string()),
    }
}
