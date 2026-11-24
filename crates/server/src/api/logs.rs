//! 只读观测接口（SPEC §8）：GET /api/devices、/api/conflicts、/api/runs、/api/ledger。
//! 均为 Admin 鉴权；Admin 页本身是 M10，本轮只做接口。

use crate::api::auth::{err, require_admin, ApiResult};
use crate::db::repo;
use crate::state::AppState;
use axum::http::{HeaderMap, StatusCode};
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

fn clamp_limit(limit: i64) -> i64 {
    limit.clamp(1, 500)
}

pub async fn devices(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<repo::DeviceRow>>> {
    require_admin(&state, &headers)?;
    let db = state
        .db
        .lock()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
    repo::list_devices(&db)
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))
}

pub async fn conflicts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Json<Vec<repo::ConflictRow>>> {
    require_admin(&state, &headers)?;
    let db = state
        .db
        .lock()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
    repo::list_conflicts(&db, clamp_limit(q.limit))
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))
}

pub async fn runs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Json<Vec<repo::RunRow>>> {
    require_admin(&state, &headers)?;
    let db = state
        .db
        .lock()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
    repo::list_runs(&db, clamp_limit(q.limit))
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))
}

pub async fn ledger(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Json<Vec<repo::LedgerRow>>> {
    require_admin(&state, &headers)?;
    let db = state
        .db
        .lock()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
    repo::list_ledger(&db, clamp_limit(q.limit))
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))
}
