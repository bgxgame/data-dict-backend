use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use crate::AppState;
use crate::models::user::Claims;
use jsonwebtoken::{decode, DecodingKey, Validation};

/// 管理员权限守卫
pub async fn guard(
    State(_state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. 提取 Authorization Header
    let auth_header = req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    if let Some(auth_header) = auth_header {
        // 2. 检查是否为 Bearer Token
        if auth_header.starts_with("Bearer ") {
            let token = &auth_header[7..];
            
            // 3. 解码并验证 JWT
            let token_data = decode::<Claims>(
                token,
                &DecodingKey::from_secret("secret_key".as_ref()),
                &Validation::default(),
            );

            if let Ok(data) = token_data {
                // 4. 只有角色为 admin 的用户才允许访问管理接口
                if data.claims.role == "admin" {
                    return Ok(next.run(req).await);
                }
                return Err(StatusCode::FORBIDDEN); // 权限不足
            }
        }
    }
    
    // 5. 未提供 Token 或 Token 无效
    Err(StatusCode::UNAUTHORIZED)
}