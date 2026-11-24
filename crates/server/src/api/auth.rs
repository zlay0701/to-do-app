//! 鉴权：同步令牌（Bearer，常量时间比较）与 Admin 会话（SPEC §8、§11）。
//!
//! 401 不区分"令牌不存在 / 不匹配"：两种情况返回同一错误。

use crate::crypto::{constant_time_eq, decrypt, encrypt, random_token};
use crate::db::repo;
use crate::state::AppState;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{extract::State, Json};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use todo_core::now_ms;

pub type ApiResult<T> = Result<T, (StatusCode, Json<ErrorBody>)>;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

pub fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<ErrorBody>) {
    (
        status,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
}

/// 从 secrets 表解密同步令牌；不存在时返回 None（调用方统一按 401 处理）。
pub fn stored_sync_token(state: &AppState) -> Option<String> {
    let db = state.db.lock().ok()?;
    let blob = repo::get_secret(&db, "sync_token").ok()??;
    let plain = decrypt(&state.master_key, &blob).ok()?;
    String::from_utf8(plain).ok()
}

pub fn verify_sync_token(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(stored) = stored_sync_token(state) else {
        return false;
    };
    let Some(bearer) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return false;
    };
    constant_time_eq(bearer.as_bytes(), stored.as_bytes())
}

pub fn require_sync(state: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    if verify_sync_token(state, headers) {
        Ok(())
    } else {
        Err(err(StatusCode::UNAUTHORIZED, "unauthorized"))
    }
}

/// 从 Cookie 取 Admin 会话令牌。
fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|part| {
                part.trim()
                    .strip_prefix("todo_admin_session=")
                    .map(|t| t.to_string())
            })
        })
}

/// Admin 会话：Cookie `todo_admin_session`，24h 过期；会话落库（M10），重启仍有效。
pub fn require_admin(state: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    let Some(token) = session_token(headers) else {
        return Err(err(StatusCode::UNAUTHORIZED, "unauthorized"));
    };
    let db = state
        .db
        .lock()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
    match repo::get_session_expiry(&db, &token) {
        Ok(Some(expiry)) if expiry > now_ms() => Ok(()),
        Ok(Some(_)) => {
            // 过期会话顺手清理掉，下次不再命中。
            let _ = repo::delete_session(&db, &token);
            Err(err(StatusCode::UNAUTHORIZED, "unauthorized"))
        }
        _ => Err(err(StatusCode::UNAUTHORIZED, "unauthorized")),
    }
}

pub fn hash_password(password: &str) -> Result<String, (StatusCode, Json<ErrorBody>)> {
    // argon2id，参数用 crate 默认（m=19MiB, t=2, p=1）。
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    let salt = SaltString::generate(&mut OsRng);
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "password hash failed"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    argon2.verify_password(password.as_bytes(), &parsed).is_ok()
}

#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

/// POST /api/login：成功置 HttpOnly + SameSite=Lax + Secure 会话 Cookie，24h。
pub async fn login(State(state): State<Arc<AppState>>, Json(input): Json<LoginInput>) -> Response {
    let admin = {
        let Ok(db) = state.db.lock() else {
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db locked").into_response();
        };
        repo::get_admin(&db)
    };
    let Ok(Some((username, hash))) = admin else {
        return err(StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    if username != input.username || !verify_password(&input.password, &hash) {
        return err(StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let token = random_token();
    let expiry = now_ms() + 24 * 60 * 60 * 1000;
    {
        let Ok(db) = state.db.lock() else {
            return err(StatusCode::INTERNAL_SERVER_ERROR, "db locked").into_response();
        };
        // 会话落库（M10）：重启后仍有效；顺手清理已过期会话。
        let _ = repo::delete_expired_sessions(&db, now_ms());
        if repo::insert_session(&db, &token, expiry).is_err() {
            return err(StatusCode::INTERNAL_SERVER_ERROR, "session store failed").into_response();
        }
    }
    let cookie = format!(
        "todo_admin_session={token}; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=86400"
    );
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

/// POST /api/logout：清会话并清 Cookie。
pub async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        if let Ok(db) = state.db.lock() {
            let _ = repo::delete_session(&db, &token);
        }
    }
    (
        StatusCode::OK,
        [(
            header::SET_COOKIE,
            "todo_admin_session=; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=0",
        )],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatus {
    pub has_token: bool,
}

/// GET /api/token：同步令牌是否已配置（不返回令牌本身）。
pub async fn token_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<TokenStatus>> {
    require_admin(&state, &headers)?;
    Ok(Json(TokenStatus {
        has_token: stored_sync_token(&state).is_some(),
    }))
}

/// POST /api/token：生成并重置同步令牌（SPEC §8：令牌在 Admin 页生成/重置）。
///
/// 新令牌只在本次响应里返回一次，调用方须立即保存；旧令牌即刻失效，
/// 已配好旧令牌的客户端须更新后才能继续同步。
pub async fn rotate_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&state, &headers)?;
    let token = random_token();
    {
        let db = state
            .db
            .lock()
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
        let enc = encrypt(&state.master_key, token.as_bytes())
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "encrypt failed"))?;
        repo::set_secret(&db, "sync_token", &enc)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    }
    Ok(Json(serde_json::json!({ "ok": true, "token": token })))
}
