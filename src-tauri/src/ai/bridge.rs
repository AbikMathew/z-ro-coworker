use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AiBridge {
    client: Client,
    api_key: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

impl AiBridge {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    pub async fn ask(&self, question: &str, task_context: &str) -> Result<String, String> {
        if self.api_key.is_empty() {
            return Ok("AI is not configured yet. Add your OpenAI API key to .env to enable the co-worker.".to_string());
        }

        let system_prompt = super::prompts::get_coworker_prompt(task_context);

        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: question.to_string(),
                },
            ],
            max_tokens: 300,
        };

        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        chat_response.choices.first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| "No response from AI".to_string())
    }
}
