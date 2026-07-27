use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::model::RpCharacter;
use crate::generation::Message;

pub struct FirestoreDb {
    client: Client,
    project_id: String,
    api_key: String,
}

// --- Firestore JSON Parsing Structures ---

#[derive(Deserialize, Debug)]
struct FirestoreListResponse {
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
    
    /// Increment the message count for a specific character
    pub async fn increment_message_count(&self, char_id: &str) -> Result<(), Box<dyn std::error::Error>> {
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