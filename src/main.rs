mod generation;
mod model;
mod database;

use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use std::sync::Arc;
use tower_http::services::ServeDir;
use tokio::net::TcpListener;
use reqwest::Client;

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

    let app = Router::new()
        // Новый API роут для получения списка персонажей
        .route("/api/characters", get(get_characters_handler))
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