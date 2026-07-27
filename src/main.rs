mod generation;
mod model;
mod database;
mod auth;

use axum::{
    extract::{State, Path, Extension},
    routing::{get, post},
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
        .route("/api/chat/:char_id", get(get_chat_history_handler).post(send_chat_message_handler))
        .route("/api/me", get(get_me_handler))
        .route_layer(middleware::from_fn_with_state(shared_state.clone(), auth::require_auth));

    // Публичные роуты
    let public_routes = Router::new()
        .route("/api/characters", get(get_characters_handler))
        .route("/api/characters/:char_id", get(get_character_handler));

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
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
}

#[derive(Deserialize)]
struct ChatMessageRequest {
    message: String,
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

// POST /api/characters — создание нового персонажа (требует авторизации)
async fn create_character_handler(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Json(payload): Json<CreateCharacterRequest>,
) -> Result<Json<model::RpCharacter>, StatusCode> {
    if payload.name.trim().is_empty() || payload.internal_prompt.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    match state.db.create_character(
        &payload.name,
        &payload.avatar_url,
        &payload.description,
        &payload.internal_prompt,
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
    match state.db.get_chat_history(&char_id, &user.uid).await {
        Ok(history) => Ok(Json(history)),
        Err(e) => {
            eprintln!("❌ Ошибка при получении истории чата: {}", e);
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
    let history = state.db.get_chat_history(&char_id, &user.uid).await.unwrap_or_default();

    let settings = GenerationSettings {
        char_prompt: character.internal_prompt.clone(),
        rules: "Оставайся в роли персонажа, отвечай живо и по существу.".to_string(),
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
    if let Err(e) = state.db.save_message(&char_id, &user.uid, "user", &payload.message).await {
        eprintln!("⚠️ Не удалось сохранить сообщение пользователя: {}", e);
    }
    if let Err(e) = state.db.save_message(&char_id, &user.uid, "assistant", &reply).await {
        eprintln!("⚠️ Не удалось сохранить ответ ассистента: {}", e);
    }

    // 5. Инкрементируем счётчик сообщений персонажа
    if let Err(e) = state.db.increment_message_count(&char_id).await {
        eprintln!("⚠️ Не удалось обновить счётчик сообщений: {}", e);
    }

    Ok(Json(ChatMessageResponse { reply }))
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