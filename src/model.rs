use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Comment {
    pub uid: String,
    pub name: String,
    pub text: String,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpCharacter {
    pub id: String,
    pub name: String,
    pub avatar_url: String, // Может быть обычным URL или data:image/...;base64,... строкой
    pub description: String,
    #[serde(skip_serializing)]
    pub internal_prompt: String,
    pub message_count: u64, // Статистика: сколько сообщений ей отправили
    pub language: String,       // "ru" | "en" | произвольная строка
    pub violence_level: String, // "mild" | "medium" | "graphic"
}