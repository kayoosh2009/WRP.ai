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
    #[serde(default)]
    pub created_by_name: String, // отображаемое имя автора (денормализовано на момент создания)
    #[serde(default)]
    pub rating_avg: f64,   // средняя оценка 0.0–5.0
    #[serde(default)]
    pub rating_count: u64, // сколько человек оценили
}

/// Тело запроса POST /api/characters/:id/rating
#[derive(Deserialize, Debug)]
pub struct SetRatingRequest {
    pub rating: u8, // 1..=5
}

/// Ответ на запрос рейтинга: обновлённое среднее + оценка текущего пользователя
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RatingInfo {
    pub rating_avg: f64,
    pub rating_count: u64,
    pub my_rating: u8, // 0, если пользователь ещё не оценивал
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

/// Общая статистика сайта для блока "Наши достижения" на главной
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SiteStats {
    pub characters_created: u64,
    pub accounts_created: u64,
    pub messages_sent: u64,
}