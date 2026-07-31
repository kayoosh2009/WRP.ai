mod generation;
mod model;
mod database;
mod auth;

use axum::{
    extract::{State, Path, Extension},
    routing::{get, post, delete},
    middleware,
    http::StatusCode,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::services::ServeDir;
use tokio::net::TcpListener;
use reqwest::Client;

use crate::auth::AuthUser;
use crate::generation::{Message, GenerationSettings};

// Состояние приложения, которое будет доступно всем роутам
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<database::FirestoreDb>,
    pub token_manager: Arc<generation::TokenManager>,
    pub http_client: Client,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    println!("🚀 Запуск WRP.ai Backend...");

    let http_client = Client::new();
    let token_manager = Arc::new(generation::TokenManager::new());
    let db = Arc::new(database::FirestoreDb::new());

    let shared_state = AppState {
        db,
        token_manager,
        http_client,
    };

    let serve_static = ServeDir::new("static").append_index_html_on_directories(true);

    // Роуты, требующие авторизации через Firebase ID-токен
    let protected_routes = Router::new()
        .route("/api/characters", post(create_character_handler))
        .route("/api/chat/:char_id", get(get_chat_history_handler).post(send_chat_message_handler).delete(delete_chat_history_handler))
        .route("/api/me", get(get_me_handler))
        .route("/api/comments", post(add_comment_handler))
        .route("/api/profile/stats", get(get_profile_stats_handler))
        .route("/api/profile/characters", get(get_profile_characters_handler))
        .route_layer(middleware::from_fn_with_state(shared_state.clone(), auth::require_auth));

    // Роуты только для админа
    let admin_routes = Router::new()
        .route("/api/admin/stats", get(get_admin_stats_handler))
        .route("/api/admin/characters/:char_id", delete(delete_character_handler))
        .route_layer(middleware::from_fn_with_state(shared_state.clone(), auth::require_admin));

    // Публичные роуты
    let public_routes = Router::new()
        .route("/api/characters", get(get_characters_handler))
        .route("/api/characters/:char_id", get(get_character_handler))
        .route("/api/firebase-config", get(get_firebase_config_handler))
        .route("/api/comments", get(get_comments_handler));

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .merge(admin_routes)
        // Fallback для раздачи статики (должен быть в конце)
        .fallback_service(serve_static)
        .with_state(shared_state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("✅ Сервер слушает на http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct CreateCharacterRequest {
    name: String,
    avatar_url: String,
    description: String,
    internal_prompt: String,
    language: String,
    violence_level: String,
}

#[derive(Deserialize)]
struct ChatMessageRequest {
    message: String,
    #[serde(default = "default_response_length")]
    response_length: String,
}

fn default_response_length() -> String {
    "medium".to_string()
}

#[derive(Serialize)]
struct ChatMessageResponse {
    reply: String,
}

#[derive(Serialize)]
struct MeResponse {
    uid: String,
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
}

#[derive(Deserialize)]
struct AddCommentRequest {
    text: String,
}

#[derive(Serialize)]
struct AdminStatsResponse {
    total_characters: usize,
    total_messages: u64,
    total_comments: usize,
    most_popular: Option<model::RpCharacter>,
}

#[derive(Serialize)]
struct FirebaseConfigResponse {
    api_key: String,
    auth_domain: String,
    project_id: String,
    storage_bucket: String,
    messaging_sender_id: String,
    app_id: String,
}

// Обработчик для GET /api/characters
async fn get_characters_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<model::RpCharacter>>, axum::http::StatusCode> {
    match state.db.get_all_characters().await {
        Ok(characters) => Ok(Json(characters)),
        Err(e) => {
            eprintln!("❌ Ошибка при получении персонажей: {}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /api/characters/:char_id — детали одного персонажа
async fn get_character_handler(
    State(state): State<AppState>,
    Path(char_id): Path<String>,
) -> Result<Json<model::RpCharacter>, StatusCode> {
    match state.db.get_character(&char_id).await {
        Ok(character) => Ok(Json(character)),
        Err(e) => {
            eprintln!("❌ Ошибка при получении персонажа {}: {}", char_id, e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

const ALLOWED_LANGUAGES: [&str; 2] = ["ru", "en"];
const ALLOWED_VIOLENCE_LEVELS: [&str; 3] = ["mild", "medium", "graphic"];

async fn create_character_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<CreateCharacterRequest>,
) -> Result<Json<model::RpCharacter>, StatusCode> {
    if payload.name.trim().is_empty() || payload.internal_prompt.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // "other" язык разрешаем как есть (произвольная строка от пользователя),
    // но ru/en должны быть строго нижним регистром, а незнакомые "служебные" значения отсекаем
    if payload.language.trim().is_empty() || payload.language.len() > 40 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !ALLOWED_VIOLENCE_LEVELS.contains(&payload.violence_level.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let _ = ALLOWED_LANGUAGES; // зарезервировано, если позже захотим строгую валидацию ru/en

    match state.db.create_character(
        &user.id_token,
        &user.uid,
        &payload.name,
        &payload.avatar_url,
        &payload.description,
        &payload.internal_prompt,
        &payload.language,
        &payload.violence_level,
    ).await {
        Ok(character) => Ok(Json(character)),
        Err(e) => {
            eprintln!("❌ Ошибка при создании персонажа: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /api/chat/:char_id — история переписки текущего пользователя с персонажем
async fn get_chat_history_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(char_id): Path<String>,
) -> Result<Json<Vec<Message>>, StatusCode> {
    match state.db.get_chat_history(&user.id_token, &char_id, &user.uid).await {
        Ok(history) => Ok(Json(history)),
        Err(e) => {
            eprintln!("❌ Ошибка при получении истории чата: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// DELETE /api/chat/:char_id — удалить всю историю переписки пользователя с персонажем
async fn delete_chat_history_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(char_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    match state.db.delete_chat_history(&user.id_token, &char_id, &user.uid).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            eprintln!("❌ Ошибка при удалении истории чата: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// POST /api/chat/:char_id — отправка сообщения и получение ответа ИИ
async fn send_chat_message_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(char_id): Path<String>,
    Json(payload): Json<ChatMessageRequest>,
) -> Result<Json<ChatMessageResponse>, StatusCode> {
    if payload.message.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 1. Достаём персонажа, чтобы взять его internal_prompt
    let character = state.db.get_character(&char_id).await.map_err(|e| {
        eprintln!("❌ Персонаж не найден {}: {}", char_id, e);
        StatusCode::NOT_FOUND
    })?;

    // 2. Достаём историю чата этого пользователя с персонажем
    let history = state.db.get_chat_history(&user.id_token, &char_id, &user.uid).await.unwrap_or_default();

    let language_instruction = match character.language.as_str() {
        "ru" => "Отвечай строго на русском языке.".to_string(),
        "en" => "Respond strictly in English.".to_string(),
        other => format!("Respond strictly in the following language: {}.", other),
    };

    let violence_instruction = match character.violence_level.as_str() {
        "mild" => "Keep the story family-friendly, avoid graphic violence or gore entirely.",
        "medium" => "Moderate intensity is allowed: conflict and peril are fine, but avoid graphic gore or extreme cruelty.",
        "graphic" => "Dark and intense themes are allowed, including graphic violence, as fits the narrative.",
        _ => "Keep the story family-friendly, avoid graphic violence or gore entirely.",
    };

    let length_instruction = match payload.response_length.as_str() {
        "short" => "Keep your response very brief: 1-2 short sentences maximum.",
        "long" => "Write a detailed, immersive response with rich description, at least a few paragraphs.",
        _ => "Keep your response moderate in length: a short paragraph or two.",
    };

    let settings = GenerationSettings {
        char_prompt: character.internal_prompt.clone(),
        rules: format!(
            "Stay in character, respond naturally and with substance. {} {} {} You may use *asterisks* for emphasis/actions, Markdown-lite style. IMPORTANT: never write your response as one dense wall of text — break it into short lines or paragraphs using actual line breaks, separating dialogue, actions, and thoughts onto their own lines where it makes sense.",
            language_instruction, violence_instruction, length_instruction
        ),
    };
    // 3. Генерируем ответ ИИ
    let reply = generation::generate_rp_response(
        &state.http_client,
        &state.token_manager,
        &payload.message,
        history,
        &settings,
    ).await.map_err(|e| {
        eprintln!("❌ Ошибка генерации ответа: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 4. Сохраняем оба сообщения в историю
    if let Err(e) = state.db.save_message(&user.id_token, &char_id, &user.uid, "user", &payload.message).await {
        eprintln!("⚠️ Не удалось сохранить сообщение пользователя: {}", e);
    }
    if let Err(e) = state.db.save_message(&user.id_token, &char_id, &user.uid, "assistant", &reply).await {
        eprintln!("⚠️ Не удалось сохранить ответ ассистента: {}", e);
    }

    // 5. Инкрементируем счётчик сообщений персонажа
    if let Err(e) = state.db.increment_message_count(&user.id_token, &char_id).await {
        eprintln!("⚠️ Не удалось обновить счётчик сообщений: {}", e);
    }

    // 6. Инкрементируем личную статистику пользователя (сколько сообщений он отправил)
    if let Err(e) = state.db.increment_user_message_count(&user.id_token, &user.uid).await {
        eprintln!("⚠️ Не удалось обновить статистику пользователя: {}", e);
    }

    Ok(Json(ChatMessageResponse { reply }))
}

// GET /api/profile/stats — статистика текущего пользователя для профиля
async fn get_profile_stats_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<model::ProfileStats>, StatusCode> {
    match state.db.get_profile_stats(&user.uid).await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            eprintln!("❌ Ошибка при получении статистики профиля: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /api/profile/characters — персонажи, созданные текущим пользователем
async fn get_profile_characters_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<model::RpCharacter>>, StatusCode> {
    match state.db.get_characters_by_owner(&user.uid).await {
        Ok(characters) => Ok(Json(characters)),
        Err(e) => {
            eprintln!("❌ Ошибка при получении персонажей профиля: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /api/me — данные текущего авторизованного пользователя
async fn get_me_handler(
    Extension(user): Extension<AuthUser>,
) -> Json<MeResponse> {
    Json(MeResponse {
        uid: user.uid,
        email: user.email,
        name: user.name,
        picture: user.picture,
    })
}

// GET /api/firebase-config — публичный конфиг для инициализации Firebase JS SDK на клиенте
async fn get_firebase_config_handler() -> Json<FirebaseConfigResponse> {
    Json(FirebaseConfigResponse {
        api_key: std::env::var("FIREBASE_API_KEY").unwrap_or_default(),
        auth_domain: std::env::var("FIREBASE_AUTH_DOMAIN").unwrap_or_default(),
        project_id: std::env::var("FIREBASE_PROJECT_ID").unwrap_or_default(),
        storage_bucket: std::env::var("FIREBASE_STORAGE_BUCKET").unwrap_or_default(),
        messaging_sender_id: std::env::var("FIREBASE_MESSAGING_SENDER_ID").unwrap_or_default(),
        app_id: std::env::var("FIREBASE_APP_ID").unwrap_or_default(),
    })
}

// GET /api/comments — публичный список комментариев о проекте
async fn get_comments_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<model::Comment>>, StatusCode> {
    match state.db.get_comments().await {
        Ok(comments) => Ok(Json(comments)),
        Err(e) => {
            eprintln!("❌ Ошибка при получении комментариев: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// POST /api/comments — оставить комментарий (требует авторизации)
async fn add_comment_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<AddCommentRequest>,
) -> Result<StatusCode, StatusCode> {
    let text = payload.text.trim();
    if text.is_empty() || text.len() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let name = user.name.clone().unwrap_or_else(|| "Anonymous".to_string());

    match state.db.add_comment(&user.id_token, &user.uid, &name, text).await {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(e) => {
            eprintln!("❌ Ошибка при добавлении комментария: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /api/admin/stats — сводная статистика (только для админа)
async fn get_admin_stats_handler(
    State(state): State<AppState>,
) -> Result<Json<AdminStatsResponse>, StatusCode> {
    let characters = state.db.get_all_characters().await.map_err(|e| {
        eprintln!("❌ Ошибка при получении статистики (персонажи): {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let comments = state.db.get_comments().await.unwrap_or_default();

    let total_messages: u64 = characters.iter().map(|c| c.message_count).sum();
    let most_popular = characters.iter().max_by_key(|c| c.message_count).cloned();

    Ok(Json(AdminStatsResponse {
        total_characters: characters.len(),
        total_messages,
        total_comments: comments.len(),
        most_popular,
    }))
}

// DELETE /api/admin/characters/:char_id — удалить персонажа (только для админа)
async fn delete_character_handler(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(char_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    match state.db.delete_character(&user.id_token, &char_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            eprintln!("❌ Ошибка при удалении персонажа: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}