use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone)]
pub struct AiBridge {
    client: Client,
    api_key: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
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

        let request = json!({
            "model": "gpt-4o-mini",
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": question }
            ],
            "max_tokens": 300
        });

        self.call_api(request).await
    }

    pub async fn analyze_screenshot(
        &self,
        base64_image: &str,
        question: &str,
        task_context: &str,
    ) -> Result<String, String> {
        if self.api_key.is_empty() {
            return Ok("AI is not configured. Add your OpenAI API key to .env".to_string());
        }

        let system_prompt = super::prompts::get_vision_prompt(task_context);

        let request = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": question
                        },
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": format!("data:image/png;base64,{}", base64_image),
                                "detail": "low"
                            }
                        }
                    ]
                }
            ],
            "max_tokens": 500
        });

        self.call_api(request).await
    }

    async fn call_api(&self, request: serde_json::Value) -> Result<String, String> {
        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(format!("API error ({}): {}", status, error_body));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        chat_response.choices.first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| "No response from AI".to_string())
    }
}
