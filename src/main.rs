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