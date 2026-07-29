use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use crate::model::{RpCharacter, Comment};
use crate::generation::Message;

pub struct FirestoreDb {
    client: Client,
    project_id: String,
    api_key: String,
}

// --- Firestore JSON Parsing Structures ---

#[derive(Deserialize, Debug, Default)]
struct FirestoreListResponse {
    #[serde(default)]
    documents: Vec<FirestoreDocument>,
}

#[derive(Deserialize, Debug)]
struct FirestoreDocument {
    #[allow(dead_code)]
    name: String,
    fields: HashMap<String, FirestoreValue>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum FirestoreValue {
    String { stringValue: String },
    Integer { integerValue: String }, // Firestore returns integers as strings
}

fn get_string_field(fields: &HashMap<String, FirestoreValue>, key: &str) -> Result<String, String> {
    match fields.get(key) {
        Some(FirestoreValue::String { stringValue }) => Ok(stringValue.clone()),
        _ => Err(format!("Missing or invalid string field: {}", key)),
    }
}

fn get_integer_field(fields: &HashMap<String, FirestoreValue>, key: &str) -> Result<i64, String> {
    match fields.get(key) {
        Some(FirestoreValue::Integer { integerValue }) => {
            integerValue.parse::<i64>().map_err(|e| e.to_string())
        }
        _ => Err(format!("Missing or invalid integer field: {}", key)),
    }
}

impl FirestoreDb {
    pub fn new() -> Self {
        let project_id = std::env::var("FIREBASE_PROJECT_ID")
            .expect("FIREBASE_PROJECT_ID must be set in .env");
        let api_key = std::env::var("FIREBASE_API_KEY")
            .expect("FIREBASE_API_KEY must be set in .env");

        println!("🔥 Firestore initialized for project: {}", project_id);

        Self {
            client: Client::new(),
            project_id,
            api_key,
        }
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }


    fn base_url(&self) -> String {
        format!(
            "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents",
            self.project_id
        )
    }

    fn parse_character(&self, char_id: &str, doc: FirestoreDocument) -> Result<RpCharacter, String> {
        Ok(RpCharacter {
            id: char_id.to_string(),
            name: get_string_field(&doc.fields, "name")?,
            avatar_url: get_string_field(&doc.fields, "avatar_url")?,
            description: get_string_field(&doc.fields, "description")?,
            internal_prompt: get_string_field(&doc.fields, "internal_prompt")?,
            message_count: get_integer_field(&doc.fields, "message_count").unwrap_or(0) as u64,
            language: get_string_field(&doc.fields, "language").unwrap_or_else(|_| "en".to_string()),
            violence_level: get_string_field(&doc.fields, "violence_level").unwrap_or_else(|_| "mild".to_string()),
        })
    }

    // Helper to parse a single chat message document
    fn parse_message(&self, doc: &FirestoreDocument) -> Result<(i64, Message), String> {
        let role = get_string_field(&doc.fields, "role")?;
        let content = get_string_field(&doc.fields, "content")?;
        let timestamp = get_integer_field(&doc.fields, "timestamp").unwrap_or(0);
        Ok((timestamp, Message { role, content }))
    }

    /// Fetch a character from Firestore by their ID
    pub async fn get_character(&self, char_id: &str) -> Result<RpCharacter, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/characters/{}?key={}",
            self.base_url(),
            char_id,
            self.api_key
        );

        println!("🔍 [DB] Fetching character {} from Firestore...", char_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore GET error: {}", err_text).into());
        }

        let doc: FirestoreDocument = response.json().await?;
        let character = self.parse_character(char_id, doc)
            .map_err(|e| format!("Failed to parse character document: {}", e))?;

        Ok(character)
    }

    pub async fn get_all_characters(&self) -> Result<Vec<RpCharacter>, Box<dyn std::error::Error>> {
        let url = format!("{}/characters?key={}", self.base_url(), self.api_key);
        println!("🔍 [DB] Fetching all characters from Firestore...");
        
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore GET ALL error: {}", err_text).into());
        }

        let list_response: FirestoreListResponse = response.json().await?;
        let mut characters = Vec::new();

        for doc in list_response.documents {
            // Извлекаем ID из полного имени документа (например, "projects/.../documents/characters/char_123")
            let char_id = doc.name.split('/').last().unwrap_or("unknown").to_string();
            
            if let Ok(character) = self.parse_character(&char_id, doc) {
                characters.push(character);
            }
        }

        Ok(characters)
    }
    
    /// Создать нового RP-персонажа. ID генерируется автоматически Firestore.
    pub async fn create_character(
        &self,
        id_token: &str,
        name: &str,
        avatar_url: &str,
        description: &str,
        internal_prompt: &str,
        language: &str,
        violence_level: &str,
    ) -> Result<RpCharacter, Box<dyn std::error::Error>> {
        let url = format!("{}/characters?key={}", self.base_url(), self.api_key);

        let body = serde_json::json!({
            "fields": {
                "name": { "stringValue": name },
                "avatar_url": { "stringValue": avatar_url },
                "description": { "stringValue": description },
                "internal_prompt": { "stringValue": internal_prompt },
                "message_count": { "integerValue": "0" },
                "language": { "stringValue": language },
                "violence_level": { "stringValue": violence_level }
            }
        });

        println!("🆕 [DB] Создаю нового персонажа: {}", name);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore CREATE error: {}", err_text).into());
        }

        let doc: FirestoreDocument = response.json().await?;
        let char_id = doc.name.split('/').last().unwrap_or("unknown").to_string();

        let character = self.parse_character(&char_id, doc)
            .map_err(|e| format!("Failed to parse created character: {}", e))?;

        Ok(character)
    }

    /// Сохранить одно сообщение в историю чата пользователя с персонажем
    pub async fn save_message(
        &self,
        id_token: &str,
        char_id: &str,
        uid: &str,
        role: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/characters/{}/chats/{}/messages?key={}",
            self.base_url(),
            char_id,
            uid,
            self.api_key
        );

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let body = serde_json::json!({
            "fields": {
                "role": { "stringValue": role },
                "content": { "stringValue": content },
                "timestamp": { "integerValue": timestamp.to_string() }
            }
        });

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore SAVE MESSAGE error: {}", err_text).into());
        }

        Ok(())
    }

    /// Получить историю чата пользователя с конкретным персонажем, отсортированную по времени
    pub async fn get_chat_history(
        &self,
        id_token: &str,
        char_id: &str,
        uid: &str,
    ) -> Result<Vec<Message>, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/characters/{}/chats/{}/messages?key={}",
            self.base_url(),
            char_id,
            uid,
            self.api_key
        );

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .send()
            .await?;
        if !response.status().is_success() {
            // Если подколлекции сообщений ещё не существует — это не ошибка, просто пустой чат
            if response.status().as_u16() == 404 {
                return Ok(Vec::new());
            }
            let err_text = response.text().await?;
            return Err(format!("Firestore GET HISTORY error: {}", err_text).into());
        }

        let list_response: FirestoreListResponse = response.json().await?;
        let mut messages: Vec<(i64, Message)> = Vec::new();

        for doc in &list_response.documents {
            if let Ok(parsed) = self.parse_message(doc) {
                messages.push(parsed);
            }
        }

        // Firestore REST list не гарантирует порядок документов — сортируем сами
        messages.sort_by_key(|(ts, _)| *ts);

        Ok(messages.into_iter().map(|(_, m)| m).collect())
    }

    /// Increment the message count for a specific character
    pub async fn increment_message_count(&self, id_token: &str, char_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Read current state to get the existing count
        let character = self.get_character(char_id).await?;
        let new_count = character.message_count + 1;

        // 2. Prepare the PATCH URL with an updateMask to only touch the message_count field
        let url = format!(
            "{}/characters/{}?key={}&updateMask.fieldPaths=message_count",
            self.base_url(),
            char_id,
            self.api_key
        );

        // 3. Format the payload exactly as Firestore REST API expects
        let body = serde_json::json!({
            "fields": {
                "message_count": {
                    "integerValue": new_count.to_string()
                }
            }
        });

        println!("📈 [DB] Updating message count for {} to {}", char_id, new_count);

        let response = self.client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore PATCH error: {}", err_text).into());
        }

        Ok(())
    }
}