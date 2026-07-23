use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// --- Структуры для Ollama API ---

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

// --- Менеджер токенов с паттерном Guard ---

pub struct TokenState {
    pub token: String,
    pub is_busy: bool,
}

pub struct TokenManager {
    tokens: Arc<Mutex<Vec<TokenState>>>,
}

impl TokenManager {
    pub fn new() -> Self {
        // Пытаемся загрузить токены из .env (через запятую: TOKEN1,TOKEN2,...)
        let env_tokens = std::env::var("OLLAMA_API_KEYS").unwrap_or_default();
        let mut states = Vec::new();

        if env_tokens.trim().is_empty() {
            // Если в .env пусто, генерируем 100 тестовых токенов, как ты просил
            for i in 1..=100 {
                states.push(TokenState {
                    token: format!("dummy_test_token_{}", i),
                    is_busy: false,
                });
            }
            println!("⚠️ OLLAMA_API_KEYS не найден в .env. Используется 100 тестовых токенов.");
        } else {
            for token in env_tokens.split(',') {
                let trimmed = token.trim().to_string();
                if !trimmed.is_empty() {
                    states.push(TokenState {
                        token: trimmed,
                        is_busy: false,
                    });
                }
            }
            println!("✅ Загружено {} токенов из .env", states.len());
        }

        Self {
            tokens: Arc::new(Mutex::new(states)),
        }
    }

    /// Пытается получить свободный токен. Возвращает Guard, который автоматически 
    /// освободит токен при удалении (выходе из области видимости или панике).
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
        None // Все токены заняты или пусты
    }
}

/// Гард, который автоматически освобождает токен при уничтожении (Drop)
pub struct TokenGuard {
    index: usize,
    #[allow(dead_code)] // Храним на случай отладки
    token: String,
    manager: Arc<Mutex<Vec<TokenState>>>,
}

impl Drop for TokenGuard {
    fn drop(&mut self) {
        if let Ok(mut tokens) = self.manager.lock() {
            if let Some(state) = tokens.get_mut(self.index) {
                state.is_busy = false;
                // println!("🔓 Токен #{} освобожден", self.index); // Можно раскомментировать для отладки
            }
        }
    }
}

// --- Основная функция генерации ---

pub struct GenerationSettings {
    pub char_prompt: String,
    pub rules: String, // "будь легким, не пиши много, действия после >>"
}

pub async fn generate_rp_response(
    client: &Client,
    token_manager: &TokenManager,
    user_input: &str,
    history: Vec<Message>,
    settings: &GenerationSettings,
) -> Result<String, Box<dyn std::error::Error>> {
    
    // 1. Пытаемся получить свободный токен
    let _guard = token_manager.acquire_token().ok_or("Все токены сейчас заняты. Попробуйте позже.")?;
    // Пока _guard существует, токен заблокирован. Он освободится в конце функции.

    // 2. Формируем системное сообщение, объединяя промпт персонажа и настройки
    let system_content = format!(
        "РОЛЬ: {}\n\nПРАВИЛА ОТВЕТА: {}\n\nОтвечай строго на русском языке, сохраняя атмосферу.",
        settings.char_prompt,
        settings.rules
    );

    // 3. Собираем полный массив сообщений (Система + История + Новое сообщение пользователя)
    let mut messages = vec![Message {
        role: "system".to_string(),
        content: system_content,
    }];
    
    messages.extend(history);
    
    messages.push(Message {
        role: "user".to_string(),
        content: user_input.to_string(),
    });

    // 4. Формируем запрос к Ollama Cloud
    let request_payload = OllamaRequest {
        model: "gemma4:cloud".to_string(), // Твоя целевая модель
        messages,
        stream: false, // Пока делаем синхронный ответ внутри асинхронной функции для простоты
    };

    // 5. Выполняем HTTP запрос
    let response = client
        .post("https://ollama.com/api/chat")
        .header("Authorization", format!("Bearer {}", _guard.token))
        .header("Content-Type", "application/json")
        .json(&request_payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let err_text = response.text().await?;
        return Err(format!("Ollama API вернул ошибку: {}", err_text).into());
    }

    // 6. Парсим ответ
    let ollama_response: OllamaResponse = response.json().await?;
    
    Ok(ollama_response.message.content)
}

// --- Пример использования (можно вынести в main.rs) ---
#[tokio::main]
async fn main() {
    // Загружаем переменные из файла .env
    dotenvy::dotenv().ok();

    let client = Client::new();
    let token_manager = TokenManager::new();

    let settings = GenerationSettings {
        char_prompt: "Ты — загадочный эльф-следопыт по имени Лириэль. Ты говоришь тихо, загадочно и немного насмешливо.".to_string(),
        rules: "Будь легкой к пользователю. Не пиши много текста (максимум 2-3 абзаца). Описывай свои действия после символа >>".to_string(),
    };

    let history = vec![
        Message { role: "user".to_string(), content: "Привет, кто ты?".to_string() },
        Message { role: "assistant".to_string(), content: ">> Легкий кивок из тени. Я Лириэль. А ты, кажется, заблудился, путник.".to_string() },
    ];

    println!("🔄 Запрос к ИИ...");
    
    match generate_rp_response(
        &client,
        &token_manager,
        "Покажи мне дорогу к древним руинам.",
        history,
        &settings,
    ).await {
        Ok(response) => {
            println!("✅ Ответ бота:\n{}", response);
        }
        Err(e) => {
            println!("❌ Ошибка: {}", e);
        }
    }
}