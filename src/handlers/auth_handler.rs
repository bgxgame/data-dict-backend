use axum::{extract::State, Json, http::StatusCode, response::IntoResponse};
use std::sync::Arc;
use crate::{AppState, models::user::{User, Claims}};
use argon2::{Argon2, PasswordHash, PasswordVerifier, password_hash::{SaltString, PasswordHasher}};
use jsonwebtoken::{encode, Header, EncodingKey};
use serde::{Deserialize, Serialize};
use chrono::Utc;
use rand::rngs::OsRng; // 修复 OsRng 引用

#[derive(Deserialize)]
pub struct AuthPayload {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub role: String,
}

/// 用户登录
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AuthPayload>,
) -> impl IntoResponse {
    // 显式映射字段，确保 password_hash 和 role 非空
    let user = sqlx::query_as!(
        User, 
        r#"SELECT id, username, password_hash as "password_hash!", role as "role!", created_at FROM users WHERE username = $1"#, 
        payload.username
    )
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some(user) = user {
        if let Ok(parsed_hash) = PasswordHash::new(&user.password_hash) {
            if Argon2::default().verify_password(payload.password.as_bytes(), &parsed_hash).is_ok() {
                let claims = Claims {
                    sub: user.id,
                    exp: (Utc::now() + chrono::Duration::hours(24)).timestamp() as usize,
                    role: user.role.clone(),
                };
                
                let token = encode(
                    &Header::default(), 
                    &claims, 
                    &EncodingKey::from_secret("secret_key".as_ref())
                ).unwrap();

                return (StatusCode::OK, Json(AuthResponse { token, role: user.role })).into_response();
            }
        }
    }
    (StatusCode::UNAUTHORIZED, "用户名或密码错误").into_response()
}

/// 用户注册
pub async fn signup(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AuthPayload>,
) -> impl IntoResponse {
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .unwrap_or_default();

    let res = sqlx::query!(
        "INSERT INTO users (username, password_hash, role) VALUES ($1, $2, $3)",
        payload.username, password_hash, "user"
    )
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(_) => (StatusCode::BAD_REQUEST, "用户已存在或数据库异常").into_response(),
    }
}