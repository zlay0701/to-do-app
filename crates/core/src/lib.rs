//! 双端共用的核心库：模型、SQLite 数据层、LWW 合并。
//! 不依赖 Tauri、不做 IO 之外的事（LWW 是纯函数，见 `lww`）。

pub mod db;
pub mod error;
pub mod lww;
pub mod models;

/// 当前毫秒时间戳（UTC）。LWW 的 `updated_at` 以它为准。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
