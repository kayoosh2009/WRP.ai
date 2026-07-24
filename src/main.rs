// Подключаем наши модули
mod generation;
mod model;
mod database;

use axum::Router;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tokio::net::TcpListener;
use reqwest::Client;

#[tokio::main]
async fn main() {
    // Загружаем .env
    dotenvy::dotenv().ok();

    println!("🚀 Запуск AI RP Backend...");

    // 1. Инициализируем общие ресурсы (они будут жить весь срок работы сервера)
    let http_client = Client::new();
    let token_manager = Arc::new(generation::TokenManager::new());
    let db = Arc::new(database::FirestoreDb::new());

    // 2. Настраиваем раздачу статических файлов
    // ServeDir::new("static") говорит Axum: "Ищи файлы в папке static"
    // Если пользователь зайдет на /, Axum автоматически попробует отдать static/index.html
    let serve_static = ServeDir::new("static").append_index_html_on_directories(true);

    let app = Router::new()
        .fallback_service(serve_static);
    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("✅ Сервер слушает на http://localhost:3000");
    println!("📂 Отдаем статику из папки 'static'");
    
    axum::serve(listener, app).await.unwrap();
}