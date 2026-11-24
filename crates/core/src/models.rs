//! 同步协议结构体只在这里定义一次（SPEC §2.2、§9）。
//! 两端 serde 序列化必须一致；字段增删先改这里 + SPEC（§16 第 8 条）。

use serde::{Deserialize, Serialize};

/// 任务状态：0 未完成，1 已完成。
pub const STATUS_ACTIVE: i32 = 0;
pub const STATUS_DONE: i32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoList {
    pub id: String,
    pub name: String,
    pub color: String,
    pub sort_order: i64,
    pub deleted_at: Option<i64>,
    pub updated_at: i64,
    pub origin: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub notes: String,
    pub status: i32,
    pub priority: i32,
    pub due_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub sort_order: f64,
    pub deleted_at: Option<i64>,
    pub updated_at: i64,
    pub origin: String,
    pub created_at: i64,
}

/// 一批待同步的记录（推 / 拉共用同一形状）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncChanges {
    #[serde(default)]
    pub lists: Vec<TodoList>,
    #[serde(default)]
    pub tasks: Vec<Task>,
}

/// POST /api/v1/sync 请求体（SPEC §9.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub last_cursor: i64,
    pub client_time: i64,
    pub changes: SyncChanges,
}

/// POST /api/v1/sync 响应体（SPEC §9.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResponse {
    pub cursor: i64,
    /// 服务端库代际标识（v4.4，§9.5）：与本地存储不一致即服务端换库，按首次同步处理。
    #[serde(default)]
    pub generation: Option<String>,
    pub server_time: i64,
    pub clock_offset_ms: i64,
    pub conflicts: i64,
    pub apply: SyncChanges,
}

/// 客户端同步状态（`sync_client_state` 表的镜像，供前端展示）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncClientState {
    pub cursor: i64,
    /// 上次见到的服务端 generation（v4.4）。
    #[serde(default)]
    pub generation: Option<String>,
    pub last_sync_at: Option<i64>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub clock_offset_ms: Option<i64>,
}
