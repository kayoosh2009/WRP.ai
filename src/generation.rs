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
    // Ollama при stream:false возвращает эти поля в корне ответа.
    // Если API их не пришлёт — по умолчанию 0, чтобы не падать.
    #[serde(default)]
    prompt_eval_count: u64,
    #[serde(default)]
    eval_count: u64,
}

pub struct TokenState {
    pub token: String,
    pub alias: String, // человекочитаемое имя ключа ("сервера"), задаётся через имя переменной в .env
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
                // Алиас — это "хвост" имени переменной после OLLAMA_API_KEY.
                // Например OLLAMA_API_KEY_MAIN -> алиас "MAIN".
                // Если хвоста нет (переменная называется просто OLLAMA_API_KEY) — даём номер.
                let suffix = key
                    .strip_prefix("OLLAMA_API_KEY")
                    .unwrap_or("")
                    .trim_start_matches('_');

                let alias = if suffix.is_empty() {
                    format!("KEY_{}", states.len() + 1)
                } else {
                    suffix.to_string()
                };

                states.push(TokenState {
                    token: value.trim().to_string(),
                    alias,
                    is_busy: false,
                });
            }
        }

        if states.is_empty() {
            println!("⚠️ Токены OLLAMA_API_KEY_* не найдены в .env. Генерация ответов будет недоступна, пока не будет добавлен хотя бы один токен.");
        } else {
            let aliases: Vec<&str> = states.iter().map(|s| s.alias.as_str()).collect();
            println!("✅ Загружено {} отдельных токенов из .env: {:?}", states.len(), aliases);
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
                    alias: state.alias.clone(),
                    manager: self.tokens.clone(),
                });
            }
        }
        None
    }
}

pub struct TokenGuard {
    index: usize,
    pub token: String,
    pub alias: String,
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

/// Сколько токенов и на каком ключе ("сервере") было потрачено за один вызов
pub struct TokenUsage {
    pub alias: String,
    pub tokens: i64,
}

/// Результат генерации: сам ответ + список трат по ключам
/// (обычно 1 запись — генерация; 2, если сработала модерация)
pub struct GenerationResult {
    pub reply: String,
    pub usage: Vec<TokenUsage>,
}

/// Структурированный черновик персонажа, который возвращает ИИ
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GeneratedCharacter {
    pub name: String,
    pub description: String,
    pub internal_prompt: String,
    pub language: String,       // "ru" | "en"
    pub violence_level: String, // "mild" | "medium" | "graphic"
}

pub struct CharacterIdeaResult {
    pub character: GeneratedCharacter,
    pub usage: Vec<TokenUsage>,
}

const CHARACTER_IDEA_SYSTEM_PROMPT: &str = "\
Ты — помощник для создания персонажей ролевого чата (RP-платформа).\n\
Тебе присылают необязательную короткую затравку от пользователя (может быть пустой).\n\
Придумай интересного, детально проработанного персонажа на основе затравки \
(или полностью случайного, если затравка пустая).\n\
\n\
Верни СТРОГО валидный JSON без markdown-разметки, без ``` и без пояснений — \
только сам JSON-объект со следующими полями:\n\
{\n\
  \"name\": \"имя персонажа, до 80 символов\",\n\
  \"description\": \"короткое описание для карточки персонажа, 1-2 предложения, до 300 символов\",\n\
  \"internal_prompt\": \"развёрнутый системный промпт: личность, манера речи, предыстория и правила поведения, 3-6 предложений\",\n\
  \"language\": \"ru или en — язык общения персонажа (определи по языку затравки, если затравка пустая — используй en)\",\n\
  \"violence_level\": \"mild, medium или graphic — насколько тёмная/интенсивная история подразумевается\"\n\
}\n\
Пиши description и internal_prompt на том же языке, что указан в поле language.";

/// Генерирует черновик персонажа (имя, описание, промпт, язык, интенсивность) по короткой затравке пользователя
pub async fn generate_character_idea(
    client: &Client,
    token_manager: &TokenManager,
    hint: &str,
) -> Result<CharacterIdeaResult, Box<dyn std::error::Error>> {
    let _guard = token_manager.acquire_token().ok_or("Все токены сейчас заняты.")?;

    let user_content = if hint.trim().is_empty() {
        "Придумай полностью случайного персонажа.".to_string()
    } else {
        format!("Затравка от пользователя: {}", hint.trim())
    };

    let messages = vec![
        Message {
            role: "system".to_string(),
            content: CHARACTER_IDEA_SYSTEM_PROMPT.to_string(),
        },
        Message {
            role: "user".to_string(),
            content: user_content,
        },
    ];

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
        return Err(format!("Ollama API ошибка (генерация персонажа): {}", err_text).into());
    }

    let ollama_response: OllamaResponse = response.json().await?;
    let raw = ollama_response.message.content;
    let tokens = (ollama_response.prompt_eval_count + ollama_response.eval_count) as i64;

    let character = parse_character_json(&raw)
        .ok_or_else(|| format!("Не удалось разобрать ответ ИИ как JSON персонажа: {}", raw))?;

    Ok(CharacterIdeaResult {
        character,
        usage: vec![TokenUsage {
            alias: _guard.alias.clone(),
            tokens,
        }],
    })
}

/// ИИ иногда оборачивает JSON в ```json ... ``` или добавляет текст вокруг — вырезаем сам объект
fn parse_character_json(raw: &str) -> Option<GeneratedCharacter> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str::<GeneratedCharacter>(&raw[start..=end]).ok()
}

pub async fn generate_rp_response(
    client: &Client,
    token_manager: &TokenManager,
    user_input: &str,
    history: Vec<Message>,
    settings: &GenerationSettings,
) -> Result<GenerationResult, Box<dyn std::error::Error>> {
    let _guard = token_manager.acquire_token().ok_or("Все токены сейчас заняты.")?;

    let system_content = format!(
        "ROLE: {}\n\nRESPONSE RULES: {}",
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
    let draft = ollama_response.message.content;
    let gen_tokens = (ollama_response.prompt_eval_count + ollama_response.eval_count) as i64;

    let mut usage = vec![TokenUsage {
        alias: _guard.alias.clone(),
        tokens: gen_tokens,
    }];

    if needs_moderation(&draft) {
        match moderate_response(client, token_manager, &draft).await {
            Ok((safe_text, mod_alias, mod_tokens)) => {
                usage.push(TokenUsage {
                    alias: mod_alias,
                    tokens: mod_tokens as i64,
                });
                Ok(GenerationResult { reply: safe_text, usage })
            }
            Err(e) => {
                eprintln!("⚠️ Модерация недоступна, отдаём черновик: {}", e);
                Ok(GenerationResult { reply: draft, usage })
            }
        }
    } else {
        Ok(GenerationResult { reply: draft, usage })
    }
}

/// Быстрая локальная проверка без сетевого запроса.
/// Если ни одно слово-триггер не найдено — пропускаем тяжёлую модерацию через LLM.
fn needs_moderation(text: &str) -> bool {
    const TRIGGER_WORDS: &[&str] = &[
        "наркотик", "нарко", "кокаин", "героин", "амфетамин", "мефедрон",
        "передозировк", "таблетк", "препарат",
        "оружи", "пистолет", "взрывчат", "бомба", "патрон",
        "суицид", "самоубийств", "порез", "вены",
        "убий", "изнасил", "похищени", "теракт",
        "несовершеннолетн", "малолетн",
    ];

    let lower = text.to_lowercase();
    TRIGGER_WORDS.iter().any(|w| lower.contains(w))
}

const MODERATION_PROMPT: &str = "\
Ты — модератор текста ролевого чата. Тебе присылают ответ ИИ-персонажа игроку.\n\
Твоя задача:\n\
1. Проверь текст на: инструкции по созданию оружия/наркотиков/взрывчатки, реальные незаконные действия с пошаговыми деталями, сексуализацию несовершеннолетних, разжигание ненависти.\n\
2. Если ничего из этого нет — верни текст БЕЗ ИЗМЕНЕНИЙ.\n\
3. Если есть — перепиши ТОЛЬКО проблемные фрагменты так, чтобы сюжет и персонаж сохранились, но опасные детали исчезли (например, замени конкретный рецепт/инструкцию на общее описание \"персонаж уклончиво отвечает\" или похожее по духу сцены действие).\n\
4. Не добавляй никаких пояснений, дисклеймеров или комментариев от себя — ответь только финальным текстом, который увидит пользователь.";

/// Возвращает (безопасный текст, алиас ключа, потрачено токенов)
async fn moderate_response(
    client: &Client,
    token_manager: &TokenManager,
    draft: &str,
) -> Result<(String, String, u64), Box<dyn std::error::Error>> {
    let _guard = token_manager.acquire_token().ok_or("Все токены сейчас заняты (модерация).")?;

    let messages = vec![
        Message {
            role: "system".to_string(),
            content: MODERATION_PROMPT.to_string(),
        },
        Message {
            role: "user".to_string(),
            content: draft.to_string(),
        },
    ];

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
        return Err(format!("Ollama API ошибка (модерация): {}", err_text).into());
    }

    let ollama_response: OllamaResponse = response.json().await?;
    let tokens = ollama_response.prompt_eval_count + ollama_response.eval_count;
    Ok((ollama_response.message.content, _guard.alias.clone(), tokens))
}