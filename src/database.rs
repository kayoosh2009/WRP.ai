use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use crate::model::{RpCharacter, Comment};
use crate::generation::Message;
use crate::model::TokenUsageStat;

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
    String { #[serde(rename = "stringValue")] string_value: String },
    Integer { #[serde(rename = "integerValue")] integer_value: String }, // Firestore returns integers as strings
    Double { #[serde(rename = "doubleValue")] double_value: f64 },
}

fn get_string_field(fields: &HashMap<String, FirestoreValue>, key: &str) -> Result<String, String> {
    match fields.get(key) {
        Some(FirestoreValue::String { string_value }) => Ok(string_value.clone()),
        _ => Err(format!("Missing or invalid string field: {}", key)),
    }
}

fn get_integer_field(fields: &HashMap<String, FirestoreValue>, key: &str) -> Result<i64, String> {
    match fields.get(key) {
        Some(FirestoreValue::Integer { integer_value }) => {
            integer_value.parse::<i64>().map_err(|e| e.to_string())
        }
        _ => Err(format!("Missing or invalid integer field: {}", key)),
    }
}

fn get_double_field(fields: &HashMap<String, FirestoreValue>, key: &str) -> Result<f64, String> {
    match fields.get(key) {
        Some(FirestoreValue::Double { double_value }) => Ok(*double_value),
        // Firestore может отдать "ровное" число (например 0 или 5) как integerValue, а не doubleValue
        Some(FirestoreValue::Integer { integer_value }) => {
            integer_value.parse::<f64>().map_err(|e| e.to_string())
        }
        _ => Err(format!("Missing or invalid double field: {}", key)),
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
            created_by: get_string_field(&doc.fields, "created_by").unwrap_or_default(),
            created_by_name: get_string_field(&doc.fields, "created_by_name").unwrap_or_else(|_| "Anonymous".to_string()),
            rating_avg: get_double_field(&doc.fields, "rating_avg").unwrap_or(0.0),
            rating_count: get_integer_field(&doc.fields, "rating_count").unwrap_or(0) as u64,
        })
    }

    // Helper to parse a single chat message document
    fn parse_message(&self, doc: &FirestoreDocument) -> Result<(i64, Message), String> {
        let role = get_string_field(&doc.fields, "role")?;
        let content = get_string_field(&doc.fields, "content")?;
        let timestamp = get_integer_field(&doc.fields, "timestamp").unwrap_or(0);
        Ok((timestamp, Message { role, content }))
    }

    fn parse_comment(&self, doc: &FirestoreDocument) -> Result<Comment, String> {
        Ok(Comment {
            uid: get_string_field(&doc.fields, "uid")?,
            name: get_string_field(&doc.fields, "name")?,
            text: get_string_field(&doc.fields, "text")?,
            timestamp: get_integer_field(&doc.fields, "timestamp").unwrap_or(0),
        })
    }

    fn parse_notification(&self, doc: &FirestoreDocument) -> Result<crate::model::Notification, String> {
        Ok(crate::model::Notification {
            id: doc.name.split('/').last().unwrap_or("unknown").to_string(),
            title: get_string_field(&doc.fields, "title")?,
            message: get_string_field(&doc.fields, "message")?,
            timestamp: get_integer_field(&doc.fields, "timestamp").unwrap_or(0),
        })
    }

    fn parse_sponsor(&self, doc: &FirestoreDocument) -> Result<crate::model::Sponsor, String> {
        Ok(crate::model::Sponsor {
            id: doc.name.split('/').last().unwrap_or("unknown").to_string(),
            name: get_string_field(&doc.fields, "name")?,
            url: get_string_field(&doc.fields, "url")?,
            timestamp: get_integer_field(&doc.fields, "timestamp").unwrap_or(0),
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
        uid: &str,
        created_by_name: &str,
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
                "violence_level": { "stringValue": violence_level },
                "created_by": { "stringValue": uid },
                "created_by_name": { "stringValue": created_by_name },
                "rating_avg": { "doubleValue": 0.0 },
                "rating_count": { "integerValue": "0" }
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

    /// Получить оценку конкретного пользователя для персонажа (0, если ещё не оценивал)
    pub async fn get_user_rating(
        &self,
        id_token: &str,
        char_id: &str,
        uid: &str,
    ) -> Result<u8, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/characters/{}/ratings/{}?key={}",
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
            return Ok(0); // документа ещё нет — значит юзер не оценивал
        }

        let doc: FirestoreDocument = response.json().await?;
        let rating = get_integer_field(&doc.fields, "rating").unwrap_or(0);
        Ok(rating as u8)
    }

    /// Поставить/изменить оценку персонажа и пересчитать среднее
    pub async fn set_rating(
        &self,
        id_token: &str,
        char_id: &str,
        uid: &str,
        rating: u8,
    ) -> Result<crate::model::RatingInfo, Box<dyn std::error::Error>> {
        // 1. Сохраняем/перезаписываем оценку конкретного пользователя
        let rating_url = format!(
            "{}/characters/{}/ratings/{}?key={}",
            self.base_url(),
            char_id,
            uid,
            self.api_key
        );

        let rating_body = serde_json::json!({
            "fields": {
                "rating": { "integerValue": rating.to_string() }
            }
        });

        let put_response = self.client
            .patch(&rating_url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&rating_body)
            .send()
            .await?;

        if !put_response.status().is_success() {
            let err_text = put_response.text().await?;
            return Err(format!("Firestore PATCH RATING error: {}", err_text).into());
        }

        // 2. Читаем все оценки этого персонажа, чтобы пересчитать среднее
        let list_url = format!(
            "{}/characters/{}/ratings?key={}",
            self.base_url(),
            char_id,
            self.api_key
        );

        let list_response = self.client
            .get(&list_url)
            .header("Authorization", format!("Bearer {}", id_token))
            .send()
            .await?;

        if !list_response.status().is_success() {
            let err_text = list_response.text().await?;
            return Err(format!("Firestore LIST RATINGS error: {}", err_text).into());
        }

        let list: FirestoreListResponse = list_response.json().await?;
        let mut sum: i64 = 0;
        let mut count: i64 = 0;
        for doc in &list.documents {
            if let Ok(r) = get_integer_field(&doc.fields, "rating") {
                sum += r;
                count += 1;
            }
        }
        let avg = if count > 0 { sum as f64 / count as f64 } else { 0.0 };

        // 3. Обновляем денормализованные rating_avg / rating_count в самом персонаже
        let char_url = format!(
            "{}/characters/{}?key={}&updateMask.fieldPaths=rating_avg&updateMask.fieldPaths=rating_count",
            self.base_url(),
            char_id,
            self.api_key
        );

        let char_body = serde_json::json!({
            "fields": {
                "rating_avg": { "doubleValue": avg },
                "rating_count": { "integerValue": count.to_string() }
            }
        });

        let char_response = self.client
            .patch(&char_url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&char_body)
            .send()
            .await?;

        if !char_response.status().is_success() {
            let err_text = char_response.text().await?;
            return Err(format!("Firestore PATCH RATING_AVG error: {}", err_text).into());
        }

        Ok(crate::model::RatingInfo {
            rating_avg: avg,
            rating_count: count as u64,
            my_rating: rating,
        })
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

    /// Список чатов пользователя: для каждого персонажа, с которым есть переписка,
    /// отдаёт последнее сообщение. Отсортировано по убыванию времени последнего сообщения
    /// (сверху — самый свежий диалог).
    pub async fn get_user_chat_summaries(
        &self,
        id_token: &str,
        uid: &str,
    ) -> Result<Vec<crate::model::ChatSummary>, Box<dyn std::error::Error>> {
        let characters = self.get_all_characters().await?;
        let mut summaries = Vec::new();

        for character in characters {
            let url = format!(
                "{}/characters/{}/chats/{}/messages?key={}",
                self.base_url(),
                character.id,
                uid,
                self.api_key
            );

            let response = self.client
                .get(&url)
                .header("Authorization", format!("Bearer {}", id_token))
                .send()
                .await?;

            if !response.status().is_success() {
                continue; // нет чата с этим персонажем — пропускаем
            }

            let list_response: FirestoreListResponse = response.json().await?;
            let mut messages: Vec<(i64, Message)> = Vec::new();
            for doc in &list_response.documents {
                if let Ok(parsed) = self.parse_message(doc) {
                    messages.push(parsed);
                }
            }
            if messages.is_empty() {
                continue;
            }
            messages.sort_by_key(|(ts, _)| *ts);
            let (last_ts, last_msg) = messages.last().unwrap().clone();

            summaries.push(crate::model::ChatSummary {
                char_id: character.id,
                char_name: character.name,
                avatar_url: character.avatar_url,
                last_message: last_msg.content,
                last_role: last_msg.role,
                last_timestamp: last_ts,
            });
        }

        summaries.sort_by(|a, b| b.last_timestamp.cmp(&a.last_timestamp));
        Ok(summaries)
    }

/// Удалить всю историю чата пользователя с конкретным персонажем
    pub async fn delete_chat_history(
        &self,
        id_token: &str,
        char_id: &str,
        uid: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
            if response.status().as_u16() == 404 {
                return Ok(());
            }
            let err_text = response.text().await?;
            return Err(format!("Firestore LIST FOR DELETE error: {}", err_text).into());
        }

        let list_response: FirestoreListResponse = response.json().await?;

        for doc in &list_response.documents {
            let delete_url = format!(
                "https://firestore.googleapis.com/v1/{}?key={}",
                doc.name,
                self.api_key
            );
            let del_resp = self.client
                .delete(&delete_url)
                .header("Authorization", format!("Bearer {}", id_token))
                .send()
                .await?;

            if !del_resp.status().is_success() {
                let err_text = del_resp.text().await?;
                eprintln!("⚠️ Не удалось удалить сообщение {}: {}", doc.name, err_text);
            }
        }

        Ok(())
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

/// Получить количество отправленных пользователем сообщений (для профиля)
    async fn get_user_message_count(&self, uid: &str) -> i64 {
        let url = format!("{}/users/{}?key={}", self.base_url(), uid, self.api_key);
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(doc) = resp.json::<FirestoreDocument>().await {
                    return get_integer_field(&doc.fields, "messages_sent").unwrap_or(0);
                }
                0
            }
            _ => 0,
        }
    }

    /// true, если Firestore-документ пользователя уже существует (не первое его сообщение)
    async fn user_doc_exists(&self, uid: &str) -> bool {
        let url = format!("{}/users/{}?key={}", self.base_url(), uid, self.api_key);
        matches!(self.client.get(&url).send().await, Ok(resp) if resp.status().is_success())
    }

    /// Увеличить счётчик отправленных пользователем сообщений на 1
    pub async fn increment_user_message_count(&self, id_token: &str, uid: &str) -> Result<(), Box<dyn std::error::Error>> {
        let is_new_user = !self.user_doc_exists(uid).await;

        let current = self.get_user_message_count(uid).await;
        let new_count = current + 1;

        let url = format!(
            "{}/users/{}?key={}&updateMask.fieldPaths=messages_sent",
            self.base_url(),
            uid,
            self.api_key
        );

        let body = serde_json::json!({
            "fields": {
                "messages_sent": { "integerValue": new_count.to_string() }
            }
        });

        let response = self.client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore PATCH USER STATS error: {}", err_text).into());
        }

        // Первое сообщение этого пользователя вообще — считаем его в глобальную статистику аккаунтов
        if is_new_user {
            if let Err(e) = self.increment_global_counter(id_token, "accounts_created").await {
                eprintln!("⚠️ Не удалось увеличить счётчик аккаунтов: {}", e);
            }
        }

        Ok(())
    }

    /// Увеличить на 1 именованное поле в публичном документе site_stats/global (best-effort счётчик)
    async fn increment_global_counter(&self, id_token: &str, field: &str) -> Result<(), Box<dyn std::error::Error>> {
        let get_url = format!("{}/site_stats/global?key={}", self.base_url(), self.api_key);
        let current = match self.client.get(&get_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let doc: FirestoreDocument = resp.json().await?;
                get_integer_field(&doc.fields, field).unwrap_or(0)
            }
            _ => 0,
        };
        let new_value = current + 1;

        let patch_url = format!(
            "{}/site_stats/global?key={}&updateMask.fieldPaths={}",
            self.base_url(),
            self.api_key,
            field
        );

        let body = serde_json::json!({
            "fields": { field: { "integerValue": new_value.to_string() } }
        });

        let response = self.client
            .patch(&patch_url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore INCREMENT COUNTER error: {}", err_text).into());
        }

        Ok(())
    }

    /// Получить статистику профиля пользователя
    pub async fn get_profile_stats(&self, uid: &str) -> Result<crate::model::ProfileStats, Box<dyn std::error::Error>> {
        let messages_sent = self.get_user_message_count(uid).await as u64;

        let characters = self.get_all_characters().await.unwrap_or_default();
        let characters_created = characters.iter().filter(|c| c.created_by == uid).count() as u64;

        Ok(crate::model::ProfileStats {
            messages_sent,
            characters_created,
            forum_messages: 0,
        })
    }

    /// Получить персонажей, созданных конкретным пользователем
    pub async fn get_characters_by_owner(&self, uid: &str) -> Result<Vec<RpCharacter>, Box<dyn std::error::Error>> {
        let characters = self.get_all_characters().await?;
        Ok(characters.into_iter().filter(|c| c.created_by == uid).collect())
    }

    /// Удалить персонажа (только для админа)
    pub async fn delete_character(&self, id_token: &str, char_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/characters/{}?key={}", self.base_url(), char_id, self.api_key);

        let response = self.client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore DELETE error: {}", err_text).into());
        }

        Ok(())
    }

    /// Добавить комментарий о проекте
    pub async fn add_comment(
        &self,
        id_token: &str,
        uid: &str,
        name: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/comments?key={}", self.base_url(), self.api_key);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let body = serde_json::json!({
            "fields": {
                "uid": { "stringValue": uid },
                "name": { "stringValue": name },
                "text": { "stringValue": text },
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
            return Err(format!("Firestore ADD COMMENT error: {}", err_text).into());
        }

        Ok(())
    }

    /// Получить все комментарии (новые сверху)
    pub async fn get_comments(&self) -> Result<Vec<Comment>, Box<dyn std::error::Error>> {
        let url = format!("{}/comments?key={}", self.base_url(), self.api_key);

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            if response.status().as_u16() == 404 {
                return Ok(Vec::new());
            }
            let err_text = response.text().await?;
            return Err(format!("Firestore GET COMMENTS error: {}", err_text).into());
        }

        let list_response: FirestoreListResponse = response.json().await?;
        let mut comments = Vec::new();

        for doc in &list_response.documents {
            if let Ok(comment) = self.parse_comment(doc) {
                comments.push(comment);
            }
        }

        comments.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // новые сверху

        Ok(comments)
    }

    /// Получить последние уведомления (новые сверху, максимум 20 штук)
    pub async fn get_notifications(&self) -> Result<Vec<crate::model::Notification>, Box<dyn std::error::Error>> {
        let url = format!("{}/notifications?key={}", self.base_url(), self.api_key);

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            if response.status().as_u16() == 404 {
                return Ok(Vec::new());
            }
            let err_text = response.text().await?;
            return Err(format!("Firestore GET NOTIFICATIONS error: {}", err_text).into());
        }

        let list_response: FirestoreListResponse = response.json().await?;
        let mut notifications = Vec::new();

        for doc in &list_response.documents {
            if let Ok(n) = self.parse_notification(doc) {
                notifications.push(n);
            }
        }

        notifications.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // новые сверху
        notifications.truncate(20);

        Ok(notifications)
    }

    /// Создать новое уведомление (только для админа)
    pub async fn add_notification(
        &self,
        id_token: &str,
        title: &str,
        message: &str,
    ) -> Result<crate::model::Notification, Box<dyn std::error::Error>> {
        let url = format!("{}/notifications?key={}", self.base_url(), self.api_key);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let body = serde_json::json!({
            "fields": {
                "title": { "stringValue": title },
                "message": { "stringValue": message },
                "timestamp": { "integerValue": timestamp.to_string() }
            }
        });

        println!("🔔 [DB] Создаю уведомление: {}", title);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore ADD NOTIFICATION error: {}", err_text).into());
        }

        let doc: FirestoreDocument = response.json().await?;
        let notification = self.parse_notification(&doc)
            .map_err(|e| format!("Failed to parse created notification: {}", e))?;

        Ok(notification)
    }

    /// Получить всех спонсоров (новые сверху)
    pub async fn get_sponsors(&self) -> Result<Vec<crate::model::Sponsor>, Box<dyn std::error::Error>> {
        let url = format!("{}/sponsors?key={}", self.base_url(), self.api_key);

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            if response.status().as_u16() == 404 {
                return Ok(Vec::new());
            }
            let err_text = response.text().await?;
            return Err(format!("Firestore GET SPONSORS error: {}", err_text).into());
        }

        let list_response: FirestoreListResponse = response.json().await?;
        let mut sponsors = Vec::new();

        for doc in &list_response.documents {
            if let Ok(s) = self.parse_sponsor(doc) {
                sponsors.push(s);
            }
        }

        sponsors.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // новые сверху

        Ok(sponsors)
    }

    /// Добавить спонсора (только для админа)
    pub async fn add_sponsor(
        &self,
        id_token: &str,
        name: &str,
        url: &str,
    ) -> Result<crate::model::Sponsor, Box<dyn std::error::Error>> {
        let firestore_url = format!("{}/sponsors?key={}", self.base_url(), self.api_key);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let body = serde_json::json!({
            "fields": {
                "name": { "stringValue": name },
                "url": { "stringValue": url },
                "timestamp": { "integerValue": timestamp.to_string() }
            }
        });

        println!("🌟 [DB] Добавляю спонсора: {}", name);

        let response = self.client
            .post(&firestore_url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore ADD SPONSOR error: {}", err_text).into());
        }

        let doc: FirestoreDocument = response.json().await?;
        let sponsor = self.parse_sponsor(&doc)
            .map_err(|e| format!("Failed to parse created sponsor: {}", e))?;

        Ok(sponsor)
    }

    /// Удалить спонсора (только для админа)
    pub async fn delete_sponsor(&self, id_token: &str, sponsor_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/sponsors/{}?key={}", self.base_url(), sponsor_id, self.api_key);

        let response = self.client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore DELETE SPONSOR error: {}", err_text).into());
        }

        Ok(())
    }

    /// Записывает трату токенов по конкретному ключу ("серверу"): и в общий счётчик,
    /// и в счётчик текущего месяца. Данные хранятся в коллекции token_usage,
    /// сам API-ключ никогда никуда не пишется — только заранее заданный алиас.
    pub async fn record_token_usage(
        &self,
        id_token: &str,
        alias: &str,
        tokens: i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if tokens <= 0 {
            return Ok(());
        }

        let doc_id = sanitize_alias_for_doc_id(alias);
        let month_field = current_month_field();

        // 1. Читаем текущие значения (если документа ещё нет — считаем их нулями)
        let get_url = format!("{}/token_usage/{}?key={}", self.base_url(), doc_id, self.api_key);
        let existing_fields = match self.client.get(&get_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                resp.json::<FirestoreDocument>().await.ok().map(|d| d.fields)
            }
            _ => None,
        };

        let current_total = existing_fields
            .as_ref()
            .and_then(|f| get_integer_field(f, "total_all_time").ok())
            .unwrap_or(0);
        let current_month_value = existing_fields
            .as_ref()
            .and_then(|f| get_integer_field(f, &month_field).ok())
            .unwrap_or(0);

        let new_total = current_total + tokens;
        let new_month_value = current_month_value + tokens;

        // 2. Пишем алиас (для читаемости) + оба счётчика одним PATCH-запросом
        let patch_url = format!(
            "{}/token_usage/{}?key={}&updateMask.fieldPaths=alias&updateMask.fieldPaths=total_all_time&updateMask.fieldPaths={}",
            self.base_url(), doc_id, self.api_key, month_field
        );

        let mut fields_map = serde_json::Map::new();
        fields_map.insert("alias".to_string(), serde_json::json!({ "stringValue": alias }));
        fields_map.insert("total_all_time".to_string(), serde_json::json!({ "integerValue": new_total.to_string() }));
        fields_map.insert(month_field, serde_json::json!({ "integerValue": new_month_value.to_string() }));

        let body = serde_json::json!({ "fields": fields_map });

        let response = self.client
            .patch(&patch_url)
            .header("Authorization", format!("Bearer {}", id_token))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Firestore RECORD TOKEN USAGE error: {}", err_text).into());
        }

        Ok(())
    }

    pub async fn get_token_usage_stats(&self, id_token: &str) -> Result<Vec<TokenUsageStat>, Box<dyn std::error::Error>> {
        let url = format!("{}/token_usage?key={}", self.base_url(), self.api_key);

        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", id_token))
            .send()
            .await?;
        if !response.status().is_success() {
            if response.status().as_u16() == 404 {
                return Ok(Vec::new());
            }
            let err_text = response.text().await?;
            return Err(format!("Firestore GET TOKEN USAGE error: {}", err_text).into());
        }

        let list_response: FirestoreListResponse = response.json().await?;
        let month_field = current_month_field();
        let mut stats = Vec::new();

        for doc in &list_response.documents {
            let alias = get_string_field(&doc.fields, "alias")
                .unwrap_or_else(|_| doc.name.split('/').last().unwrap_or("unknown").to_string());
            let total_all_time = get_integer_field(&doc.fields, "total_all_time").unwrap_or(0);
            let current_month = get_integer_field(&doc.fields, &month_field).unwrap_or(0);

            stats.push(TokenUsageStat {
                alias,
                total_all_time,
                current_month,
            });
        }

        stats.sort_by(|a, b| b.total_all_time.cmp(&a.total_all_time));

        Ok(stats)
    }

    /// Общая статистика сайта: сколько всего персонажей, аккаунтов и отправленных сообщений
    pub async fn get_site_stats(&self) -> Result<crate::model::SiteStats, Box<dyn std::error::Error>> {
        let characters = self.get_all_characters().await.unwrap_or_default();
        let characters_created = characters.len() as u64;
        // messages_sent берём из публично читаемого message_count каждого персонажа —
        // не нужно лезть в закрытую коллекцию /users
        let messages_sent: u64 = characters.iter().map(|c| c.message_count).sum();

        let url = format!("{}/site_stats/global?key={}", self.base_url(), self.api_key);
        let accounts_created = match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<FirestoreDocument>().await {
                    Ok(doc) => get_integer_field(&doc.fields, "accounts_created").unwrap_or(0) as u64,
                    Err(_) => 0,
                }
            }
            _ => 0, // документа ещё нет — значит ни одного сообщения ещё не отправляли
        };

        Ok(crate::model::SiteStats {
            characters_created,
            accounts_created,
            messages_sent,
        })
    }
}

fn sanitize_alias_for_doc_id(alias: &str) -> String {
    let cleaned: String = alias
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "unnamed".to_string()
    } else {
        cleaned
    }
}

/// Имя поля в Firestore для текущего месяца, например "month_2026_08"
fn current_month_field() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let (year, month) = year_month_from_millis(ms);
    format!("month_{:04}_{:02}", year, month)
}

/// (год, месяц) из UNIX-времени в миллисекундах, без внешних крейтов вроде chrono.
/// Стандартный алгоритм civil_from_days (Howard Hinnant), работает для григорианского календаря.
fn year_month_from_millis(ms: i64) -> (i64, u32) {
    let days = ms.div_euclid(86_400_000);
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m: i64 = if mp < 10 { mp as i64 + 3 } else { mp as i64 - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32)
}