use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::model::RpCharacter;

pub struct FirestoreDb {
    client: Client,
    project_id: String,
    api_key: String,
}

// --- Firestore JSON Parsing Structures ---

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

    fn base_url(&self) -> String {
        format!(
            "https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents",
            self.project_id
        )
    }

    // Helper to parse the complex Firestore JSON response into our RpCharacter struct
    fn parse_character(&self, char_id: &str, doc: FirestoreDocument) -> Result<RpCharacter, String> {
        let get_string = |key: &str| -> Result<String, String> {
            match doc.fields.get(key) {
                Some(FirestoreValue::String { stringValue }) => Ok(stringValue.clone()),
                _ => Err(format!("Missing or invalid string field: {}", key)),
            }
        };

        let get_integer = |key: &str| -> Result<u64, String> {
            match doc.fields.get(key) {
                Some(FirestoreValue::Integer { integerValue }) => {
                    integerValue.parse::<u64>().map_err(|e| e.to_string())
                }
                _ => Err(format!("Missing or invalid integer field: {}", key)),
            }
        };

        Ok(RpCharacter {
            id: char_id.to_string(),
            name: get_string("name")?,
            avatar_url: get_string("avatar_url")?,
            description: get_string("description")?,
            internal_prompt: get_string("internal_prompt")?,
            message_count: get_integer("message_count").unwrap_or(0),
        })
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