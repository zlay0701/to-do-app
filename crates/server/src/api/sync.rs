//! 同步中枢（SPEC §9）：POST /api/v1/sync、GET /api/v1/ping。
//!
//! 合并逐条走 `core/lww.rs` 的 decide_*；每次写入在事务内 bump rev_counter。
//! 限频：每设备最小间隔 5 秒，违反 429 TOO_FREQUENT。

use crate::api::auth::{err, require_sync, ApiResult};
use crate::db::repo;
use crate::state::AppState;
use axum::http::{HeaderMap, StatusCode};
use axum::{extract::State, Json};
use std::sync::Arc;
use todo_core::lww::{decide_list, decide_task, LwwOutcome};
use todo_core::models::{SyncChanges, SyncRequest, SyncResponse, Task, TodoList};
use todo_core::now_ms;

const MAX_PULL: i64 = 5000;
const MIN_SYNC_INTERVAL_MS: i64 = 5000;

pub async fn ping(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    require_sync(&state, &headers)?;
    Ok(Json(
        serde_json::json!({ "ok": true, "serverTime": now_ms() }),
    ))
}

pub async fn sync(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<SyncRequest>,
) -> ApiResult<Json<SyncResponse>> {
    require_sync(&state, &headers)?;
    if req.device_id.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "deviceId required"));
    }
    let now = now_ms();
    let server_time = now;
    let clock_offset_ms = req.client_time - server_time;

    // 限频先于合并：按设备上次同步时间判定。
    {
        let db = state
            .db
            .lock()
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
        if let Ok(Some(last)) = repo::device_last_sync_at(&db, &req.device_id) {
            if now - last < MIN_SYNC_INTERVAL_MS {
                return Err(err(StatusCode::TOO_MANY_REQUESTS, "TOO_FREQUENT"));
            }
        }
    }

    let (conflicts, rejected_lists, rejected_tasks) = merge_changes(&state, &req, now)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    // 拉取：server_rev > lastCursor，单次最多 5000 条；cursor 取本轮拉到的最大 rev
    //（截断时只能推进到最后一条，否则会丢记录；无记录时沿用原 cursor）。
    let (mut lists, mut tasks, cursor) = {
        let db = state
            .db
            .lock()
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
        let pulled_lists = repo::pull_lists(&db, req.last_cursor, MAX_PULL)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
        let remaining = MAX_PULL - pulled_lists.len() as i64;
        let pulled_tasks = if remaining > 0 {
            repo::pull_tasks(&db, req.last_cursor, remaining)
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        } else {
            Vec::new()
        };
        let cursor = pulled_lists
            .iter()
            .map(|(_, rev)| *rev)
            .chain(pulled_tasks.iter().map(|(_, rev)| *rev))
            .max()
            .unwrap_or(req.last_cursor);
        let lists: Vec<TodoList> = pulled_lists.into_iter().map(|(l, _)| l).collect();
        let tasks: Vec<Task> = pulled_tasks.into_iter().map(|(t, _)| t).collect();
        (lists, tasks, cursor)
    };

    // v4.7（§9.3 方案 A）：被拒记录的服务端权威版本也随 apply 下发（去重，
    // 不因此推进 cursor——cursor 已按拉取的最大 rev 定好）。
    // 否则客户端按步骤 3 清 dirty 后仍留被拒旧内容、后续同步不再收敛（静默留旧）。
    for list in rejected_lists {
        if !lists.iter().any(|l| l.id == list.id) {
            lists.push(list);
        }
    }
    for task in rejected_tasks {
        if !tasks.iter().any(|t| t.id == task.id) {
            tasks.push(task);
        }
    }

    {
        let db = state
            .db
            .lock()
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
        repo::upsert_device(
            &db,
            &req.device_id,
            &req.device_name,
            &req.platform,
            cursor,
            now,
            Some(clock_offset_ms),
            now,
        )
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    }

    // v4.4：代际标识随响应下发，客户端比对后决定是否全量重同步（§9.5）。
    let generation = {
        let db = state
            .db
            .lock()
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
        repo::get_generation(&db)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
    };

    Ok(Json(SyncResponse {
        cursor,
        generation,
        server_time,
        clock_offset_ms,
        conflicts,
        apply: SyncChanges { lists, tasks },
    }))
}

/// 合并全部推来的记录（一个事务内完成；每条写入前 bump rev_counter）。
/// 返回（本轮被拒绝的条数，被拒记录的服务端权威版本：列表，任务）。
/// 权威版本由调用方放进 apply 下发（v4.7 §9.3 方案 A），否则客户端清 dirty 后
/// 仍留被拒旧内容、后续同步不再收敛。
fn merge_changes(
    state: &Arc<AppState>,
    req: &SyncRequest,
    now: i64,
) -> Result<(i64, Vec<TodoList>, Vec<Task>), rusqlite::Error> {
    let mut db = state.db.lock().map_err(|_| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            Some("db locked".to_string()),
        )
    })?;
    let tx = db.transaction()?;
    let mut conflicts: i64 = 0;
    let mut rejected_lists: Vec<TodoList> = Vec::new();
    let mut rejected_tasks: Vec<Task> = Vec::new();

    for list in &req.changes.lists {
        match repo::get_list_tx(&tx, &list.id)? {
            None => {
                let rev = repo::next_rev(&tx)?;
                repo::upsert_list_tx(&tx, list, rev)?;
            }
            Some((existing, _rev)) => {
                let (outcome, log) = decide_list(list, &existing);
                match outcome {
                    LwwOutcome::Accept => {
                        let rev = repo::next_rev(&tx)?;
                        repo::upsert_list_tx(&tx, list, rev)?;
                        if log {
                            repo::log_conflict_tx(
                                &tx,
                                "list",
                                &list.id,
                                &list.origin,
                                &existing.origin,
                                list.updated_at,
                                existing.updated_at,
                                now,
                            )?;
                        }
                    }
                    LwwOutcome::Reject => {
                        conflicts += 1;
                        if log {
                            repo::log_conflict_tx(
                                &tx,
                                "list",
                                &list.id,
                                &existing.origin,
                                &list.origin,
                                existing.updated_at,
                                list.updated_at,
                                now,
                            )?;
                        }
                        rejected_lists.push(existing);
                    }
                    LwwOutcome::Ignore => {}
                }
            }
        }
    }

    for task in &req.changes.tasks {
        match repo::get_task_tx(&tx, &task.id)? {
            None => {
                let rev = repo::next_rev(&tx)?;
                repo::upsert_task_tx(&tx, task, rev)?;
            }
            Some((existing, _rev)) => {
                let (outcome, log) = decide_task(task, &existing);
                match outcome {
                    LwwOutcome::Accept => {
                        let rev = repo::next_rev(&tx)?;
                        repo::upsert_task_tx(&tx, task, rev)?;
                        if log {
                            repo::log_conflict_tx(
                                &tx,
                                "task",
                                &task.id,
                                &task.origin,
                                &existing.origin,
                                task.updated_at,
                                existing.updated_at,
                                now,
                            )?;
                        }
                    }
                    LwwOutcome::Reject => {
                        conflicts += 1;
                        if log {
                            repo::log_conflict_tx(
                                &tx,
                                "task",
                                &task.id,
                                &existing.origin,
                                &task.origin,
                                existing.updated_at,
                                task.updated_at,
                                now,
                            )?;
                        }
                        rejected_tasks.push(existing);
                    }
                    LwwOutcome::Ignore => {}
                }
            }
        }
    }

    tx.commit()?;
    Ok((conflicts, rejected_lists, rejected_tasks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::encrypt;
    use crate::db;
    use axum::http::{header, HeaderMap};
    use todo_core::models::{SyncChanges, SyncRequest, Task, TodoList};

    fn test_state() -> (Arc<AppState>, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "todo-sync-93-{}-{}.db",
            std::process::id(),
            now_ms()
        ));
        let conn = db::open(&path).unwrap();
        let master_key = [7u8; 32];
        let enc = encrypt(&master_key, b"test-token").unwrap();
        repo::set_secret(&conn, "sync_token", &enc).unwrap();
        (AppState::new(conn, master_key, 60), path)
    }

    fn headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Bearer test-token".parse().unwrap());
        h
    }

    fn req(device: &str, cursor: i64, changes: SyncChanges) -> SyncRequest {
        SyncRequest {
            device_id: device.to_string(),
            device_name: device.to_string(),
            platform: "test".to_string(),
            last_cursor: cursor,
            client_time: now_ms(),
            changes,
        }
    }

    fn list(id: &str, updated_at: i64) -> TodoList {
        TodoList {
            id: id.to_string(),
            name: "L".to_string(),
            color: "#888888".to_string(),
            sort_order: 1,
            deleted_at: None,
            updated_at,
            origin: "dev-a".to_string(),
            created_at: updated_at,
        }
    }

    fn task(id: &str, title: &str, updated_at: i64, origin: &str) -> Task {
        Task {
            id: id.to_string(),
            list_id: "L1".to_string(),
            title: title.to_string(),
            notes: String::new(),
            status: 0,
            priority: 0,
            due_at: None,
            completed_at: None,
            sort_order: 1.0,
            deleted_at: None,
            updated_at,
            origin: origin.to_string(),
            created_at: updated_at,
        }
    }

    async fn call(state: &Arc<AppState>, request: SyncRequest) -> todo_core::models::SyncResponse {
        let Json(resp) = sync(State(state.clone()), headers(), Json(request))
            .await
            .unwrap();
        resp
    }

    /// §9.3 / v4.7：被拒记录的服务端权威版本必须随 apply 下发。
    ///
    /// 客户端已拉过胜出版本（其 server_rev <= cursor），之后离线用偏慢时钟
    /// 改同一条再推：服务端拒收（conflicts=1），但旧实现只回条数、apply 为空，
    /// 客户端清 dirty 后仍留旧内容、后续同步不再收敛（与 cursor 丢数据同类）。
    /// 修复后 apply 必须包含权威版本，且 cursor 不因此推进。
    #[tokio::test]
    async fn rejected_record_authoritative_version_in_apply() {
        let (state, path) = test_state();
        let t0 = now_ms();
        // 1. dev-a 建 L1 + T1(Y)，服务端收下，cursor=2。
        let r1 = call(
            &state,
            req(
                "dev-a",
                0,
                SyncChanges {
                    lists: vec![list("L1", t0)],
                    tasks: vec![task("T1", "Y", t0, "dev-a")],
                },
            ),
        )
        .await;
        assert_eq!(r1.cursor, 2);
        // 2. dev-b 全量拉取，持有胜出版本。
        let r2 = call(&state, req("dev-b", 0, SyncChanges::default())).await;
        assert_eq!(r2.cursor, 2);
        assert!(r2
            .apply
            .tasks
            .iter()
            .any(|t| t.id == "T1" && t.title == "Y"));
        // 3. dev-b 偏慢时钟改同一条再推（限频 5s，须等）。
        tokio::time::sleep(std::time::Duration::from_millis(5100)).await;
        let r3 = call(
            &state,
            req(
                "dev-b",
                2,
                SyncChanges {
                    lists: vec![],
                    tasks: vec![task("T1", "X-old", t0 - 60_000, "dev-b")],
                },
            ),
        )
        .await;
        assert_eq!(r3.conflicts, 1);
        // cursor 不因此推进。
        assert_eq!(r3.cursor, 2);
        // 权威版本必须随 apply 下发（修复前这里是空的，测试失败即复现 bug）。
        let authoritative = r3.apply.tasks.iter().find(|t| t.id == "T1");
        assert!(
            authoritative.is_some(),
            "apply 应包含被拒记录的服务端权威版本"
        );
        assert_eq!(authoritative.unwrap().title, "Y");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
