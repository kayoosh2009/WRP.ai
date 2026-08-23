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
    #[serde(default)]
    pub created_by: String, // uid пользователя, создавшего персонажа
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileStats {
    pub messages_sent: u64,
    pub characters_created: u64,
    pub forum_messages: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub title: String,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Sponsor {
    pub id: String,
    pub name: String,
    pub url: String,
    pub timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenUsageStat {
    pub alias: String,        // название сервера/ключа, которое ты сам задал
    pub total_all_time: i64,  // потрачено токенов за всё время
    pub current_month: i64,   // потрачено токенов в текущем месяце
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatSummary {
    pub char_id: String,
    pub char_name: String,
    pub avatar_url: String,
    pub last_message: String,
    pub last_role: String,   // "user" | "assistant" — чтобы фронт мог показать "You: ..." или просто текст
    pub last_timestamp: i64,
}