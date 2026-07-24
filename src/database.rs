use reqwest::Client;
use crate::model::RpCharacter;

pub struct FirestoreDb {
    client: Client,
    project_id: String,
    // database_id: String, // Обычно это "(default)"
}

impl FirestoreDb {
    pub fn new() -> Self {
        // В .env тебе нужно будет добавить FIREBASE_PROJECT_ID
        let project_id = std::env::var("FIREBASE_PROJECT_ID").unwrap_or_else(|_| "my-project-id".to_string());
        
        Self {
            client: Client::new(),
            project_id,
        }
    }

    /// Получить персонажа из Firebase по его ID
    pub async fn get_character(&self, char_id: &str) -> Result<RpCharacter, Box<dyn std::error::Error>> {
        // TODO: Здесь будет GET запрос к Firestore REST API
        // URL будет примерно таким: 
        // https://firestore.googleapis.com/v1/projects/{project_id}/databases/(default)/documents/characters/{char_id}
        
        println!("🔍 [DB] Запрос персонажа {} из Firebase...", char_id);
        
        // Пока что возвращаем заглушку, чтобы код компилился
        Ok(RpCharacter {
            id: char_id.to_string(),
            name: "Лириэль".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
            description: "Загадочный эльф".to_string(),
            internal_prompt: "Ты эльф-следопыт...".to_string(),
            message_count: 42,
        })
    }

    /// Увеличить счетчик сообщений в Firebase
    pub async fn increment_message_count(&self, char_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Здесь будет PATCH/POST запрос для обновления поля message_count
        println!("📈 [DB] Обновление счетчика сообщений для {} в Firebase", char_id);
        Ok(())
    }
}