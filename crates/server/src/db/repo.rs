//! 服务端 SQL 唯一入口（SPEC §2.2、§4）。
//!
//! 服务端记录 = 客户端记录 + `server_rev`；`models.rs` 的结构体不带 rev，
//! 这里用 `(TodoList, i64)` / `(Task, i64)` 元组携带。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use todo_core::models::{Task, TodoList};

pub fn row_to_list(row: &rusqlite::Row) -> rusqlite::Result<TodoList> {
    Ok(TodoList {
        id: row.get("id")?,
        name: row.get("name")?,
        color: row.get("color")?,
        sort_order: row.get("sort_order")?,
        deleted_at: row.get("deleted_at")?,
        updated_at: row.get("updated_at")?,
        origin: row.get("origin")?,
        created_at: row.get("created_at")?,
    })
}

pub fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get("id")?,
        list_id: row.get("list_id")?,
        title: row.get("title")?,
        notes: row.get("notes")?,
        status: row.get("status")?,
        priority: row.get("priority")?,
        due_at: row.get("due_at")?,
        completed_at: row.get("completed_at")?,
        sort_order: row.get("sort_order")?,
        deleted_at: row.get("deleted_at")?,
        updated_at: row.get("updated_at")?,
        origin: row.get("origin")?,
        created_at: row.get("created_at")?,
    })
}

const LIST_COLS: &str =
    "id, name, color, sort_order, deleted_at, updated_at, origin, created_at, server_rev";
const TASK_COLS: &str = "id, list_id, title, notes, status, priority, due_at, completed_at, \
    sort_order, deleted_at, updated_at, origin, created_at, server_rev";

/// 库代际标识（v4.4，§9.5）：清库重建后是新值，客户端凭它发现服务端换库。
pub fn get_generation(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = 'generation'", [], |r| {
        r.get(0)
    })
    .optional()
}

/// 事务内 bump `rev_counter` 并返回新值（SPEC §4：每次写入先 bump 再写记录）。
pub fn next_rev(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<i64> {
    tx.execute(
        "UPDATE meta SET value = CAST(value AS INTEGER) + 1 WHERE key = 'rev_counter'",
        [],
    )?;
    tx.query_row(
        "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'rev_counter'",
        [],
        |r| r.get(0),
    )
}

pub fn get_list_tx(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
) -> rusqlite::Result<Option<(TodoList, i64)>> {
    tx.query_row(
        &format!("SELECT {LIST_COLS} FROM lists WHERE id = ?1"),
        params![id],
        |r| Ok((row_to_list(r)?, r.get("server_rev")?)),
    )
    .optional()
}

pub fn get_task_tx(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
) -> rusqlite::Result<Option<(Task, i64)>> {
    tx.query_row(
        &format!("SELECT {TASK_COLS} FROM tasks WHERE id = ?1"),
        params![id],
        |r| Ok((row_to_task(r)?, r.get("server_rev")?)),
    )
    .optional()
}

pub fn upsert_list_tx(
    tx: &rusqlite::Transaction<'_>,
    list: &TodoList,
    rev: i64,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO lists (id, name, color, sort_order, deleted_at, updated_at, origin, server_rev, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, color = excluded.color,
           sort_order = excluded.sort_order, deleted_at = excluded.deleted_at,
           updated_at = excluded.updated_at, origin = excluded.origin,
           server_rev = excluded.server_rev, created_at = excluded.created_at",
        params![
            list.id,
            list.name,
            list.color,
            list.sort_order,
            list.deleted_at,
            list.updated_at,
            list.origin,
            rev,
            list.created_at
        ],
    )?;
    Ok(())
}

pub fn upsert_task_tx(
    tx: &rusqlite::Transaction<'_>,
    task: &Task,
    rev: i64,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO tasks (id, list_id, title, notes, status, priority, due_at, completed_at,
                            sort_order, deleted_at, updated_at, origin, server_rev, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(id) DO UPDATE SET list_id = excluded.list_id, title = excluded.title,
           notes = excluded.notes, status = excluded.status, priority = excluded.priority,
           due_at = excluded.due_at, completed_at = excluded.completed_at,
           sort_order = excluded.sort_order, deleted_at = excluded.deleted_at,
           updated_at = excluded.updated_at, origin = excluded.origin,
           server_rev = excluded.server_rev, created_at = excluded.created_at",
        params![
            task.id,
            task.list_id,
            task.title,
            task.notes,
            task.status,
            task.priority,
            task.due_at,
            task.completed_at,
            task.sort_order,
            task.deleted_at,
            task.updated_at,
            task.origin,
            rev,
            task.created_at
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn log_conflict_tx(
    tx: &rusqlite::Transaction<'_>,
    entity: &str,
    entity_id: &str,
    winner_origin: &str,
    loser_origin: &str,
    winner_updated_at: i64,
    loser_updated_at: i64,
    now: i64,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO conflict_log (entity, entity_id, winner_origin, loser_origin,
           winner_updated_at, loser_updated_at, resolved_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entity,
            entity_id,
            winner_origin,
            loser_origin,
            winner_updated_at,
            loser_updated_at,
            now
        ],
    )?;
    Ok(())
}

/// 拉取：`server_rev > cursor`，按 rev 升序（客户端按此顺序应用，列表先于任务）。
/// 返回 `(记录, server_rev)`，调用方据此算本轮 cursor（截断时也不能越过最后一条）。
pub fn pull_lists(
    conn: &Connection,
    cursor: i64,
    limit: i64,
) -> rusqlite::Result<Vec<(TodoList, i64)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {LIST_COLS} FROM lists WHERE server_rev > ?1 ORDER BY server_rev ASC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(params![cursor, limit], |r| {
            Ok((row_to_list(r)?, r.get("server_rev")?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn pull_tasks(
    conn: &Connection,
    cursor: i64,
    limit: i64,
) -> rusqlite::Result<Vec<(Task, i64)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE server_rev > ?1 ORDER BY server_rev ASC LIMIT ?2"
    ))?;
    let rows = stmt
        .query_map(params![cursor, limit], |r| {
            Ok((row_to_task(r)?, r.get("server_rev")?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// -- 设备与限频 --------------------------------------------------------------

pub fn device_last_sync_at(conn: &Connection, device_id: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT last_sync_at FROM devices WHERE device_id = ?1",
        params![device_id],
        |r| r.get(0),
    )
    .optional()
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_device(
    conn: &Connection,
    device_id: &str,
    name: &str,
    platform: &str,
    cursor: i64,
    last_sync_at: i64,
    clock_offset_ms: Option<i64>,
    created_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO devices (device_id, name, platform, last_cursor, last_sync_at, clock_offset_ms, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(device_id) DO UPDATE SET name = excluded.name, platform = excluded.platform,
           last_cursor = excluded.last_cursor, last_sync_at = excluded.last_sync_at,
           clock_offset_ms = excluded.clock_offset_ms",
        params![device_id, name, platform, cursor, last_sync_at, clock_offset_ms, created_at],
    )?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub last_cursor: i64,
    pub last_sync_at: Option<i64>,
    pub clock_offset_ms: Option<i64>,
}

pub fn list_devices(conn: &Connection) -> rusqlite::Result<Vec<DeviceRow>> {
    let mut stmt = conn.prepare(
        "SELECT device_id, name, platform, last_cursor, last_sync_at, clock_offset_ms
         FROM devices ORDER BY last_sync_at DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DeviceRow {
                device_id: r.get("device_id")?,
                name: r.get("name")?,
                platform: r.get("platform")?,
                last_cursor: r.get("last_cursor")?,
                last_sync_at: r.get("last_sync_at")?,
                clock_offset_ms: r.get("clock_offset_ms")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// -- 冲突 / 运行 / 台账 -------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRow {
    pub id: i64,
    pub entity: String,
    pub entity_id: String,
    pub winner_origin: String,
    pub loser_origin: String,
    pub winner_updated_at: i64,
    pub loser_updated_at: i64,
    pub resolved_at: i64,
}

pub fn list_conflicts(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<ConflictRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, entity, entity_id, winner_origin, loser_origin,
                winner_updated_at, loser_updated_at, resolved_at
         FROM conflict_log ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(ConflictRow {
                id: r.get("id")?,
                entity: r.get("entity")?,
                entity_id: r.get("entity_id")?,
                winner_origin: r.get("winner_origin")?,
                loser_origin: r.get("loser_origin")?,
                winner_updated_at: r.get("winner_updated_at")?,
                loser_updated_at: r.get("loser_updated_at")?,
                resolved_at: r.get("resolved_at")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRow {
    pub id: i64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub scanned: i64,
    pub enqueued: i64,
    pub sent: i64,
    pub failed: i64,
    pub note: Option<String>,
}

pub fn list_runs(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<RunRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, started_at, finished_at, scanned, enqueued, sent, failed, note
         FROM runs ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(RunRow {
                id: r.get("id")?,
                started_at: r.get("started_at")?,
                finished_at: r.get("finished_at")?,
                scanned: r.get("scanned")?,
                enqueued: r.get("enqueued")?,
                sent: r.get("sent")?,
                failed: r.get("failed")?,
                note: r.get("note")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerRow {
    pub id: i64,
    pub task_id: String,
    pub kind: String,
    pub anchor: i64,
    pub offset_min: i64,
    pub channel: String,
    pub status: i64,
    pub attempts: i64,
    pub sent_at: Option<i64>,
    pub last_error: Option<String>,
}

pub fn list_ledger(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<LedgerRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, kind, anchor, offset_min, channel, status, attempts, sent_at, last_error
         FROM reminder_ledger ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(LedgerRow {
                id: r.get("id")?,
                task_id: r.get("task_id")?,
                kind: r.get("kind")?,
                anchor: r.get("anchor")?,
                offset_min: r.get("offset_min")?,
                channel: r.get("channel")?,
                status: r.get("status")?,
                attempts: r.get("attempts")?,
                sent_at: r.get("sent_at")?,
                last_error: r.get("last_error")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn last_run_at(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    conn.query_row("SELECT MAX(finished_at) FROM runs", [], |r| r.get(0))
}

/// 幂等认领：唯一键冲突时 `INSERT OR IGNORE` 影响 0 行 → 已有人认领过，不重发。
/// 成功返回台账 id，失败（已存在）返回 None。
pub fn claim_ledger(
    conn: &Connection,
    task_id: &str,
    kind: &str,
    anchor: i64,
    offset_min: i64,
    channel: &str,
    now: i64,
) -> rusqlite::Result<Option<i64>> {
    let affected = conn.execute(
        "INSERT OR IGNORE INTO reminder_ledger
           (task_id, kind, anchor, offset_min, channel, status, attempts, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6)",
        params![task_id, kind, anchor, offset_min, channel, now],
    )?;
    if affected == 1 {
        let id = conn.last_insert_rowid();
        Ok(Some(id))
    } else {
        Ok(None)
    }
}

pub fn set_ledger_status(
    conn: &Connection,
    id: i64,
    status: i64,
    sent_at: Option<i64>,
    last_error: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE reminder_ledger SET status = ?1, attempts = attempts + 1,
           sent_at = COALESCE(?2, sent_at), last_error = ?3 WHERE id = ?4",
        params![status, sent_at, last_error, id],
    )?;
    Ok(())
}

pub fn insert_run(conn: &Connection, started_at: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO runs (started_at) VALUES (?1)",
        params![started_at],
    )?;
    Ok(conn.last_insert_rowid())
}

#[allow(clippy::too_many_arguments)]
pub fn finish_run(
    conn: &Connection,
    id: i64,
    finished_at: i64,
    scanned: i64,
    enqueued: i64,
    sent: i64,
    failed: i64,
    note: Option<&str>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE runs SET finished_at = ?1, scanned = ?2, enqueued = ?3,
           sent = ?4, failed = ?5, note = ?6 WHERE id = ?7",
        params![finished_at, scanned, enqueued, sent, failed, note, id],
    )?;
    Ok(())
}

/// 提醒候选：未完成、未删除、有到期时间（SPEC §10 第 1 步）。
pub fn due_tasks(conn: &Connection) -> rusqlite::Result<Vec<Task>> {
    let mut stmt = conn.prepare(
        "SELECT id, list_id, title, notes, status, priority, due_at, completed_at,
                sort_order, deleted_at, updated_at, origin, created_at
         FROM tasks WHERE status = 0 AND deleted_at IS NULL AND due_at IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], row_to_task)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// -- 配置与密钥 ---------------------------------------------------------------

pub fn get_config(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM config WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
}

pub fn set_config(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_secret(conn: &Connection, key: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    conn.query_row(
        "SELECT value_enc FROM secrets WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
}

pub fn set_secret(conn: &Connection, key: &str, value_enc: &[u8]) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO secrets (key, value_enc) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_enc = excluded.value_enc",
        params![key, value_enc],
    )?;
    Ok(())
}

pub fn delete_secret(conn: &Connection, key: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM secrets WHERE key = ?1", params![key])?;
    Ok(())
}

pub fn get_admin(conn: &Connection) -> rusqlite::Result<Option<(String, String)>> {
    conn.query_row(
        "SELECT username, password_hash FROM admin WHERE id = 1",
        [],
        |r| Ok((r.get("username")?, r.get("password_hash")?)),
    )
    .optional()
}

pub fn set_admin(
    conn: &Connection,
    username: &str,
    password_hash: &str,
    created_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO admin (id, username, password_hash, created_at) VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET username = excluded.username,
           password_hash = excluded.password_hash",
        params![username, password_hash, created_at],
    )?;
    Ok(())
}

// -- Admin 会话（M10 落库持久化；内存表重启即丢，已废弃） --------------------

pub fn insert_session(conn: &Connection, token: &str, expires_at: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sessions (token, expires_at) VALUES (?1, ?2)
         ON CONFLICT(token) DO UPDATE SET expires_at = excluded.expires_at",
        params![token, expires_at],
    )?;
    Ok(())
}

pub fn get_session_expiry(conn: &Connection, token: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT expires_at FROM sessions WHERE token = ?1",
        params![token],
        |r| r.get(0),
    )
    .optional()
}

pub fn delete_session(conn: &Connection, token: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
    Ok(())
}

pub fn delete_expired_sessions(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", params![now])?;
    Ok(())
}
