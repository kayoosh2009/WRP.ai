use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: Message,
}

pub struct TokenState {
    pub token: String,
    pub is_busy: bool,
}

pub struct TokenManager {
    tokens: Arc<Mutex<Vec<TokenState>>>,
}

impl TokenManager {
    pub fn new() -> Self {
        let mut states = Vec::new();

        // Сканируем все переменные окружения
        for (key, value) in std::env::vars() {
            // Берем только те, что начинаются с OLLAMA_API_KEY и не пустые
            if key.starts_with("OLLAMA_API_KEY") && !value.trim().is_empty() {
                states.push(TokenState {
                    token: value.trim().to_string(),
                    is_busy: false,
                });
            }
        }

        if states.is_empty() {
            // Если в .env вообще нет таких ключей, генерируем 100 тестовых
            for i in 1..=100 {
                states.push(TokenState {
                    token: format!("dummy_test_token_{}", i),
                    is_busy: false,
                });
            }
            println!("⚠️ Токены OLLAMA_API_KEY_* не найдены в .env. Используется 100 тестовых.");
        } else {
            println!("✅ Загружено {} отдельных токенов из .env", states.len());
        }

        Self {
            tokens: Arc::new(Mutex::new(states)),
        }
    }

    pub fn acquire_token(&self) -> Option<TokenGuard> {
        let mut tokens = self.tokens.lock().unwrap();
        for (i, state) in tokens.iter_mut().enumerate() {
            if !state.is_busy && !state.token.is_empty() {
                state.is_busy = true;
                return Some(TokenGuard {
                    index: i,
                    token: state.token.clone(),
                    manager: self.tokens.clone(),
                });
            }
        }
        None
    }
}

pub struct TokenGuard {
    index: usize,
    #[allow(dead_code)]
    token: String,
    manager: Arc<Mutex<Vec<TokenState>>>,
}

impl Drop for TokenGuard {
    fn drop(&mut self) {
        if let Ok(mut tokens) = self.manager.lock() {
            if let Some(state) = tokens.get_mut(self.index) {
                state.is_busy = false;
            }
        }
    }
}

pub struct GenerationSettings {
    pub char_prompt: String,
    pub rules: String,
}

pub async fn generate_rp_response(
    client: &Client,
    token_manager: &TokenManager,
    user_input: &str,
    history: Vec<Message>,
    settings: &GenerationSettings,
) -> Result<String, Box<dyn std::error::Error>> {
    let _guard = token_manager.acquire_token().ok_or("Все токены сейчас заняты.")?;

    let system_content = format!(
        "РОЛЬ: {}\n\nПРАВИЛА ОТВЕТА: {}\n\nОтвечай строго на русском языке.",
        settings.char_prompt, settings.rules
    );

    let mut messages = vec![Message {
        role: "system".to_string(),
        content: system_content,
    }];
    messages.extend(history);
    messages.push(Message {
        role: "user".to_string(),
        content: user_input.to_string(),
    });

    let request_payload = OllamaRequest {
        model: "gemma4:cloud".to_string(),
        messages,
        stream: false,
    };

    let response = client
        .post("https://ollama.com/api/chat")
        .header("Authorization", format!("Bearer {}", _guard.token))
        .header("Content-Type", "application/json")
        .json(&request_payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let err_text = response.text().await?;
        return Err(format!("Ollama API ошибка: {}", err_text).into());
    }

    let ollama_response: OllamaResponse = response.json().await?;
    Ok(ollama_response.message.content)
}