use reqwest::Client;
use serde::Deserialize;
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation, Algorithm};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use axum::{
    extract::{State, Request},
    http::StatusCode,
    middleware::Next,
    response::Response,
};

use crate::AppState;

const GOOGLE_JWKS_URL: &str =
    "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com";

#[derive(Deserialize, Debug, Clone)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
}

#[derive(Deserialize, Debug)]
struct JwkSet {
    keys: Vec<Jwk>,
}

#[derive(Deserialize, Debug, Clone)]
struct FirebaseClaims {
    sub: String,
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
    aud: String,
    iss: String,
    exp: usize,
}

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub uid: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

struct CertsCache {
    keys: HashMap<String, Jwk>,
    fetched_at: Instant,
}

static CERTS_CACHE: Mutex<Option<CertsCache>> = Mutex::new(None);

async fn get_google_keys(client: &Client) -> Result<HashMap<String, Jwk>, String> {
    {
        let cache = CERTS_CACHE.lock().unwrap();
        if let Some(c) = cache.as_ref() {
            if c.fetched_at.elapsed() < Duration::from_secs(3600) {
                return Ok(c.keys.clone());
            }
        }
    }

    let resp = client
        .get(GOOGLE_JWKS_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let jwk_set: JwkSet = resp.json().await.map_err(|e| e.to_string())?;

    let mut keys = HashMap::new();
    for jwk in jwk_set.keys {
        keys.insert(jwk.kid.clone(), jwk);
    }

    let mut cache = CERTS_CACHE.lock().unwrap();
    *cache = Some(CertsCache {
        keys: keys.clone(),
        fetched_at: Instant::now(),
    });

    Ok(keys)
}

/// Проверяет Firebase ID-токен (пришедший из Google Sign-In на фронте)
pub async fn verify_firebase_token(
    client: &Client,
    project_id: &str,
    token: &str,
) -> Result<AuthUser, String> {
    let header = decode_header(token).map_err(|e| format!("Bad token header: {}", e))?;
    let kid = header.kid.ok_or("Token missing kid")?;

    let keys = get_google_keys(client).await?;
    let jwk = keys.get(&kid).ok_or("Unknown key id (kid), попробуйте повторно войти")?;

    let decoding_key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
        .map_err(|e| format!("Invalid key: {}", e))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[project_id]);
    validation.set_issuer(&[format!("https://securetoken.google.com/{}", project_id)]);

    let token_data = decode::<FirebaseClaims>(token, &decoding_key, &validation)
        .map_err(|e| format!("Token verification failed: {}", e))?;

    let claims = token_data.claims;

    Ok(AuthUser {
        uid: claims.sub,
        email: claims.email,
        name: claims.name,
        picture: claims.picture,
    })
}

/// Axum middleware: требует валидный Authorization: Bearer <token>, кладёт AuthUser в extensions
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    match verify_firebase_token(&state.http_client, state.db.project_id(), token).await {
        Ok(user) => {
            req.extensions_mut().insert(user);
            Ok(next.run(req).await)
        }
        Err(e) => {
            eprintln!("❌ Auth error: {}", e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}