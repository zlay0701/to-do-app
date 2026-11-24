//! 本地通知（M4，SPEC §5.1）：应用前台时，到期按本机偏好提醒。
//!
//! 本轮只做前台提醒：Rust 侧算出到期任务，前端每 30 秒轮询一次，
//! 到期即在应用内横幅提醒，并尝试系统 Notification API。
//! 后台推送（服务端邮件 / Webhook）是第 3 轮的事。

use todo_core::db::repo::Repo;
use todo_core::models::Task;
use todo_core::now_ms;

/// 返回已到提醒点的任务（due_at <= now + 提前量）。
pub fn due_notifications(repo: &Repo, before_min: i64) -> Vec<Task> {
    repo.due_for_reminder(now_ms(), before_min)
        .unwrap_or_default()
}
