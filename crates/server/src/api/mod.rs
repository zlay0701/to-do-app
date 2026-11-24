pub mod auth;
pub mod config;
pub mod logs;
pub mod sync;

use crate::db::repo;
use crate::state::AppState;
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
use axum::{extract::State, Json, Router};
use std::sync::Arc;
use todo_core::now_ms;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/admin", get(admin_page))
        .route("/admin/", get(admin_page))
        .route("/api/v1/ping", get(sync::ping))
        .route("/api/v1/sync", post(sync::sync))
        .route("/api/login", post(auth::login))
        .route("/api/logout", post(auth::logout))
        .route(
            "/api/token",
            get(auth::token_status).post(auth::rotate_token),
        )
        .route(
            "/api/notify",
            get(config::get_notify).put(config::put_notify),
        )
        .route("/api/test/mail", post(config::test_mail))
        .route("/api/test/webhook", post(config::test_webhook))
        .route("/api/devices", get(logs::devices))
        .route("/api/conflicts", get(logs::conflicts))
        .route("/api/runs", get(logs::runs))
        .route("/api/ledger", get(logs::ledger))
        .with_state(state)
}

/// GET /admin：内嵌单页 Admin（SPEC §2.1、M10）。
///
/// 单文件 HTML + 原生 JS，用 include_str! 编进二进制，不引第二套前端构建链。
/// 登录态由浏览器 Cookie 携带，页面自己调 /api/login 校验。
async fn admin_page() -> Html<&'static str> {
    Html(include_str!("../admin.html"))
}

/// GET /healthz（公开）：{ ok, lastRunAt, lagSeconds }。
async fn healthz(State(state): State<Arc<AppState>>) -> (StatusCode, Json<serde_json::Value>) {
    let now = now_ms();
    let last_run_at = {
        let Ok(db) = state.db.lock() else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "ok": false })),
            );
        };
        repo::last_run_at(&db).unwrap_or(None)
    };
    let lag_seconds = last_run_at.map(|t| (now - t) / 1000);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "lastRunAt": last_run_at,
            "lagSeconds": lag_seconds,
        })),
    )
}
