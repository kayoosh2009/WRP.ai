use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpCharacter {
    pub id: String,
    pub name: String,
    pub avatar_url: String,
    pub description: String,
    #[serde(skip_serializing)]
    pub internal_prompt: String,
    pub message_count: u64, // Статистика: сколько сообщений ей отправили
}