//! 客户端 SQL 唯一写入口（SPEC §2.2、§3）。
//!
//! 约定：任何本地写操作必须同时 `updated_at = now`、`dirty = 1`、
//! `origin = 本机 deviceId`。所有方法都经由本文件的写路径，禁止绕过——
//! 前端与同步代码只调 `Repo` 的方法，不直接碰 `Connection`。

use crate::error::{CoreError, Result};
use crate::models::{SyncChanges, SyncClientState, Task, TodoList, STATUS_ACTIVE, STATUS_DONE};
use crate::now_ms;
use rusqlite::{params, Connection, OptionalExtension};

pub const INBOX_ID: &str = "inbox";
pub const INBOX_NAME: &str = "收件箱";
pub const SETTING_DEVICE_ID: &str = "device_id";

/// 任务的部分更新：`None` 的字段保持原值。
#[derive(Debug, Default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub priority: Option<i32>,
    pub due_at: Option<Option<i64>>,
    pub list_id: Option<String>,
}

/// 每个列表的任务计数（供侧边栏角标；同时经 IPC 序列化给前端）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCount {
    pub list_id: String,
    pub active: i64,
    pub done: i64,
}

/// 一次 apply_sync 的结果：孤儿任务被跳过几条、是哪些（v4.4）。
#[derive(Debug, Default)]
pub struct ApplySyncOutcome {
    pub skipped_tasks: usize,
    pub skipped_task_ids: Vec<String>,
}

/// generation 是否变化到需要全量重同步的程度（SPEC §9.5，v4.4）。
/// 服务端换库 → 新 generation；本地已有 generation 且不一致 → 变化；
/// 本地无 generation 但 cursor>0（旧库第一次遇到带 generation 的服务端）→ 视为变化；
/// 本地 cursor=0（首次同步）或服务端没给 generation → 不变化，只记录。
pub fn generation_mismatch(saved: &Option<String>, incoming: &Option<String>, cursor: i64) -> bool {
    match (saved, incoming) {
        (Some(old), Some(new)) => old != new,
        (None, Some(_)) => cursor > 0,
        _ => false,
    }
}

pub struct Repo {
    conn: Connection,
    device_id: String,
}

fn row_to_list(row: &rusqlite::Row) -> rusqlite::Result<TodoList> {
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

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
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

/// 首次启动插入"收件箱"（dirty=1，等首次同步上推）。
/// 用固定 id，避免重复打开时重复插入；已被删过的收件箱不再复活。
pub fn ensure_inbox(conn: &Connection) -> Result<()> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM lists", [], |r| r.get(0))?;
    if total == 0 {
        let now = now_ms();
        conn.execute(
            "INSERT INTO lists (id, name, color, sort_order, dirty, updated_at, origin, created_at)
             VALUES (?1, ?2, '#378ADD', 0, 1, ?3, '', ?3)",
            params![INBOX_ID, INBOX_NAME, now],
        )?;
    }
    Ok(())
}

impl Repo {
    pub fn new(conn: Connection) -> Result<Self> {
        let device_id = load_or_create_device_id(&conn)?;
        Ok(Self { conn, device_id })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    // -- 列表 ---------------------------------------------------------------

    pub fn list_lists(&self) -> Result<Vec<TodoList>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color, sort_order, deleted_at, updated_at, origin, created_at
             FROM lists WHERE deleted_at IS NULL ORDER BY sort_order, created_at",
        )?;
        let rows = stmt
            .query_map([], row_to_list)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_list(&self, id: &str) -> Result<TodoList> {
        self.conn
            .query_row(
                "SELECT id, name, color, sort_order, deleted_at, updated_at, origin, created_at
                 FROM lists WHERE id = ?1",
                params![id],
                row_to_list,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("列表 {id}")))
    }

    pub fn create_list(&mut self, name: &str, color: &str) -> Result<TodoList> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::Invalid("列表名不能为空".to_string()));
        }
        let now = now_ms();
        let sort_order: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM lists",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let list = TodoList {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            color: color.to_string(),
            sort_order,
            deleted_at: None,
            updated_at: now,
            origin: self.device_id.clone(),
            created_at: now,
        };
        self.conn.execute(
            "INSERT INTO lists (id, name, color, sort_order, dirty, updated_at, origin, created_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?5)",
            params![
                list.id,
                list.name,
                list.color,
                list.sort_order,
                now,
                list.origin
            ],
        )?;
        Ok(list)
    }

    pub fn update_list(
        &mut self,
        id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<TodoList> {
        let mut list = self.get_list(id)?;
        if list.deleted_at.is_some() {
            return Err(CoreError::NotFound(format!("列表 {id} 已删除")));
        }
        if let Some(name) = name {
            let name = name.trim();
            if name.is_empty() {
                return Err(CoreError::Invalid("列表名不能为空".to_string()));
            }
            list.name = name.to_string();
        }
        if let Some(color) = color {
            list.color = color.to_string();
        }
        self.touch_list(&list)?;
        Ok(list)
    }

    /// 软删除列表：同时软删除其下全部任务（同一写语义，各自打 dirty）。
    pub fn delete_list(&mut self, id: &str) -> Result<()> {
        if id == INBOX_ID {
            return Err(CoreError::Forbidden("收件箱不能删除".to_string()));
        }
        let list = self.get_list(id)?;
        if list.deleted_at.is_some() {
            return Ok(());
        }
        let now = now_ms();
        let origin = self.device_id.clone();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE lists SET deleted_at = ?1, updated_at = ?1, dirty = 1, origin = ?2 WHERE id = ?3",
            params![now, origin, id],
        )?;
        tx.execute(
            "UPDATE tasks SET deleted_at = ?1, updated_at = ?1, dirty = 1, origin = ?2
             WHERE list_id = ?3 AND deleted_at IS NULL",
            params![now, origin, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 硬删除只允许已同步（dirty=0）的记录（SPEC §3）；否则拒绝，防止丢同步。
    pub fn hard_delete_list(&mut self, id: &str) -> Result<()> {
        let dirty: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM lists WHERE id = ?1 AND dirty = 1",
            params![id],
            |r| r.get(0),
        )?;
        let dirty_tasks: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE list_id = ?1 AND dirty = 1",
            params![id],
            |r| r.get(0),
        )?;
        if dirty > 0 || dirty_tasks > 0 {
            return Err(CoreError::Forbidden(
                "有未同步的修改，不能硬删除".to_string(),
            ));
        }
        self.conn
            .execute("DELETE FROM lists WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn reorder_list(&mut self, id: &str, sort_order: i64) -> Result<()> {
        let list = self.get_list(id)?;
        let mut list = list;
        list.sort_order = sort_order;
        self.touch_list(&list)
    }

    // -- 任务 ---------------------------------------------------------------

    pub fn list_tasks(&self, list_id: &str) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, list_id, title, notes, status, priority, due_at, completed_at,
                    sort_order, deleted_at, updated_at, origin, created_at
             FROM tasks WHERE list_id = ?1 AND deleted_at IS NULL
             ORDER BY status, sort_order, created_at",
        )?;
        let rows = stmt
            .query_map(params![list_id], row_to_task)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_task(&self, id: &str) -> Result<Task> {
        self.conn
            .query_row(
                "SELECT id, list_id, title, notes, status, priority, due_at, completed_at,
                        sort_order, deleted_at, updated_at, origin, created_at
                 FROM tasks WHERE id = ?1",
                params![id],
                row_to_task,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("任务 {id}")))
    }

    pub fn create_task(
        &mut self,
        list_id: &str,
        title: &str,
        notes: &str,
        priority: i32,
        due_at: Option<i64>,
    ) -> Result<Task> {
        let title = title.trim();
        if title.is_empty() {
            return Err(CoreError::Invalid("任务标题不能为空".to_string()));
        }
        self.get_list(list_id)?; // 列表不存在直接报错
        let now = now_ms();
        let sort_order: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks WHERE list_id = ?1 AND deleted_at IS NULL",
                params![list_id],
                |r| r.get(0),
            )
            .unwrap_or(1.0);
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            list_id: list_id.to_string(),
            title: title.to_string(),
            notes: notes.to_string(),
            status: STATUS_ACTIVE,
            priority,
            due_at,
            completed_at: None,
            sort_order,
            deleted_at: None,
            updated_at: now,
            origin: self.device_id.clone(),
            created_at: now,
        };
        self.conn.execute(
            "INSERT INTO tasks (id, list_id, title, notes, status, priority, due_at, completed_at,
                                sort_order, dirty, updated_at, origin, created_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, NULL, ?7, 1, ?8, ?9, ?8)",
            params![
                task.id,
                task.list_id,
                task.title,
                task.notes,
                task.priority,
                task.due_at,
                task.sort_order,
                now,
                task.origin,
            ],
        )?;
        Ok(task)
    }

    pub fn update_task(&mut self, id: &str, patch: TaskPatch) -> Result<Task> {
        let mut task = self.get_task(id)?;
        if task.deleted_at.is_some() {
            return Err(CoreError::NotFound(format!("任务 {id} 已删除")));
        }
        if let Some(title) = patch.title {
            let title = title.trim();
            if title.is_empty() {
                return Err(CoreError::Invalid("任务标题不能为空".to_string()));
            }
            task.title = title.to_string();
        }
        if let Some(notes) = patch.notes {
            task.notes = notes;
        }
        if let Some(priority) = patch.priority {
            task.priority = priority;
        }
        if let Some(due_at) = patch.due_at {
            task.due_at = due_at;
        }
        if let Some(list_id) = patch.list_id {
            self.get_list(&list_id)?;
            task.list_id = list_id;
        }
        self.touch_task(&task)?;
        Ok(task)
    }

    pub fn toggle_task(&mut self, id: &str) -> Result<Task> {
        let mut task = self.get_task(id)?;
        if task.deleted_at.is_some() {
            return Err(CoreError::NotFound(format!("任务 {id} 已删除")));
        }
        if task.status == STATUS_DONE {
            task.status = STATUS_ACTIVE;
            task.completed_at = None;
        } else {
            task.status = STATUS_DONE;
            task.completed_at = Some(now_ms());
        }
        self.touch_task(&task)?;
        Ok(task)
    }

    pub fn delete_task(&mut self, id: &str) -> Result<()> {
        let task = self.get_task(id)?;
        if task.deleted_at.is_some() {
            return Ok(());
        }
        let now = now_ms();
        self.conn.execute(
            "UPDATE tasks SET deleted_at = ?1, updated_at = ?1, dirty = 1, origin = ?2 WHERE id = ?3",
            params![now, self.device_id, id],
        )?;
        Ok(())
    }

    pub fn hard_delete_task(&mut self, id: &str) -> Result<()> {
        let task = self.get_task(id)?;
        if task.deleted_at.is_none() {
            return Err(CoreError::Forbidden("只能硬删除已软删除的记录".to_string()));
        }
        let dirty: i64 =
            self.conn
                .query_row("SELECT dirty FROM tasks WHERE id = ?1", params![id], |r| {
                    r.get(0)
                })?;
        if dirty == 1 {
            return Err(CoreError::Forbidden(
                "有未同步的修改，不能硬删除".to_string(),
            ));
        }
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn reorder_task(&mut self, id: &str, sort_order: f64) -> Result<()> {
        let mut task = self.get_task(id)?;
        task.sort_order = sort_order;
        self.touch_task(&task)
    }

    pub fn search_tasks(&self, query: &str, limit: i64) -> Result<Vec<Task>> {
        let q = query.trim().replace(['%', '_'], " ");
        let pattern = format!("%{q}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, list_id, title, notes, status, priority, due_at, completed_at,
                    sort_order, deleted_at, updated_at, origin, created_at
             FROM tasks WHERE deleted_at IS NULL AND (title LIKE ?1 OR notes LIKE ?1)
             ORDER BY updated_at DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![pattern, limit], row_to_task)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // -- 计数与同步读口 ------------------------------------------------------

    pub fn list_counts(&self) -> Result<Vec<ListCount>> {
        let mut stmt = self.conn.prepare(
            "SELECT list_id,
                    SUM(CASE WHEN status = 0 THEN 1 ELSE 0 END) AS active,
                    SUM(CASE WHEN status = 1 THEN 1 ELSE 0 END) AS done
             FROM tasks WHERE deleted_at IS NULL GROUP BY list_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ListCount {
                    list_id: r.get("list_id")?,
                    active: r.get("active")?,
                    done: r.get("done")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 未同步的记录条数（列表 + 任务），供状态区"本地修改 N 条待同步"展示。
    pub fn pending_dirty_count(&self) -> Result<i64> {
        let lists: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM lists WHERE dirty = 1", [], |r| {
                    r.get(0)
                })?;
        let tasks: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM tasks WHERE dirty = 1", [], |r| {
                    r.get(0)
                })?;
        Ok(lists + tasks)
    }

    /// 收集全部 dirty 记录（M5 同步引擎的输入；本轮只读不推）。
    pub fn dirty_changes(&self) -> Result<SyncChanges> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color, sort_order, deleted_at, updated_at, origin, created_at
             FROM lists WHERE dirty = 1",
        )?;
        let lists = stmt
            .query_map([], row_to_list)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut stmt = self.conn.prepare(
            "SELECT id, list_id, title, notes, status, priority, due_at, completed_at,
                    sort_order, deleted_at, updated_at, origin, created_at
             FROM tasks WHERE dirty = 1",
        )?;
        let tasks = stmt
            .query_map([], row_to_task)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(SyncChanges { lists, tasks })
    }

    /// 前台本地提醒的候选：未完成、未删除、到期时间落在
    /// `[now - 24h, now + 提醒提前量]` 内的任务。去重与"只提醒一次"由调用方按会话维护。
    pub fn due_for_reminder(&self, now: i64, before_min: i64) -> Result<Vec<Task>> {
        let horizon = now + before_min * 60_000;
        let floor = now - 24 * 60 * 60_000;
        let mut stmt = self.conn.prepare(
            "SELECT id, list_id, title, notes, status, priority, due_at, completed_at,
                    sort_order, deleted_at, updated_at, origin, created_at
             FROM tasks
             WHERE status = 0 AND deleted_at IS NULL AND due_at IS NOT NULL
               AND due_at <= ?1 AND due_at >= ?2 ORDER BY due_at",
        )?;
        let rows = stmt
            .query_map(params![horizon, floor], row_to_task)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 同步统一写入口（SPEC §9.3，M5）：在一个事务内完成三步——
    /// 已推送的记录（本地 updated_at 不大于推送值时清 dirty，往返期间有新改动则保留）、
    /// 拉取的记录（本地 dirty 且更新则保留本地，否则以服务端权威态覆盖并清 dirty，
    /// dirty/origin/updated_at 原样写入，不打新戳）、写 cursor 与同步状态。
    /// apply 里孤儿任务的处理（v4.4，方案 a）：列表先于任务应用；仍缺列表的任务跳过，
    /// 且整轮 cursor 保持同步前的值（apply 不带 server_rev，无法精确停在"第一条被跳过记录
    /// 的 rev 之前"，保守做法是整轮不推进，下轮从旧 cursor 重拉——重拉幂等），
    /// 状态记 error 而不是 ok，否则丢数据静默无察。返回跳过的孤儿任务情况供调用方展示。
    pub fn apply_sync(
        &mut self,
        pushed: &SyncChanges,
        apply: &SyncChanges,
        prev_cursor: i64,
        state: &SyncClientState,
    ) -> Result<ApplySyncOutcome> {
        let tx = self.conn.transaction()?;
        for list in &pushed.lists {
            tx.execute(
                "UPDATE lists SET dirty = 0 WHERE id = ?1 AND updated_at <= ?2",
                params![list.id, list.updated_at],
            )?;
        }
        for task in &pushed.tasks {
            tx.execute(
                "UPDATE tasks SET dirty = 0 WHERE id = ?1 AND updated_at <= ?2",
                params![task.id, task.updated_at],
            )?;
        }
        for list in &apply.lists {
            let keep_local = match get_list_tx(&tx, &list.id)? {
                Some(local) => {
                    dirty_of_list_tx(&tx, &list.id)? && local.updated_at > list.updated_at
                }
                None => false,
            };
            if !keep_local {
                upsert_list_tx(&tx, list)?;
            }
        }
        let mut skipped_ids: Vec<String> = Vec::new();
        for task in &apply.tasks {
            let keep_local = match get_task_tx(&tx, &task.id)? {
                Some(local) => {
                    dirty_of_task_tx(&tx, &task.id)? && local.updated_at > task.updated_at
                }
                None => false,
            };
            if keep_local {
                continue;
            }
            let list_exists: i64 = tx.query_row(
                "SELECT COUNT(*) FROM lists WHERE id = ?1",
                params![task.list_id],
                |r| r.get(0),
            )?;
            if list_exists == 0 {
                // 孤儿任务：所属列表本批没拉到（或服务端换库后列表 rev 已在游标之前）。
                // 只跳过、不推进 cursor——下轮重新拉取时列表在批内就不再是孤儿。
                skipped_ids.push(task.id.clone());
                continue;
            }
            upsert_task_tx(&tx, task)?;
        }
        // 有跳过时 cursor 不得推进过被跳过的记录：保持同步前的值；
        // 状态记 error 并写明条数，调用方据此向用户展示，而不是显示 ok。
        let (cursor, last_result, last_error) = if skipped_ids.is_empty() {
            (
                state.cursor,
                state.last_result.clone(),
                state.last_error.clone(),
            )
        } else {
            (
                prev_cursor,
                Some("error".to_string()),
                Some(format!(
                    "{} 条任务因所属列表未同步而暂存，下次同步重试",
                    skipped_ids.len()
                )),
            )
        };
        tx.execute(
            "INSERT INTO sync_client_state (id, cursor, generation, last_sync_at, last_result, last_error, clock_offset_ms)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET cursor = excluded.cursor,
               generation = excluded.generation,
               last_sync_at = excluded.last_sync_at, last_result = excluded.last_result,
               last_error = excluded.last_error, clock_offset_ms = excluded.clock_offset_ms",
            params![
                cursor,
                state.generation,
                state.last_sync_at,
                last_result,
                last_error,
                state.clock_offset_ms,
            ],
        )?;
        tx.commit()?;
        Ok(ApplySyncOutcome {
            skipped_tasks: skipped_ids.len(),
            skipped_task_ids: skipped_ids,
        })
    }

    /// 全部记录重新标记 dirty（v4.4）："按首次同步处理"时调用——服务端换库后，
    /// 本地已同步过的干净记录在新库里并不存在，只重推 dirty 会让新库永久缺失它们
    /// （第二台设备同步时也拉不到）。全部重推一次后 LWW 自然收敛，之后恢复增量。
    /// 软删除的 tombstone 也一并重推（dirty_changes 不分 deleted_at），删除语义不丢。
    pub fn mark_all_dirty(&mut self) -> Result<()> {
        self.conn.execute("UPDATE lists SET dirty = 1", [])?;
        self.conn.execute("UPDATE tasks SET dirty = 1", [])?;
        Ok(())
    }

    /// 重置同步（v4.4，§9.5 兜底入口）：cursor 与 generation 清零，本地全部记录
    /// 重新标记 dirty（下次同步按首次同步全量重拉、全部重推，与 generation 触发的
    /// 重置同义；原有 dirty 自然包含在内）。
    pub fn reset_sync_state(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO sync_client_state (id, cursor, generation, last_sync_at, last_result, last_error, clock_offset_ms)
             VALUES (1, 0, NULL, NULL, NULL, NULL, NULL)
             ON CONFLICT(id) DO UPDATE SET cursor = 0, generation = NULL,
               last_sync_at = NULL, last_result = NULL, last_error = NULL, clock_offset_ms = NULL",
            [],
        )?;
        tx.execute("UPDATE lists SET dirty = 1", [])?;
        tx.execute("UPDATE tasks SET dirty = 1", [])?;
        tx.commit()?;
        Ok(())
    }

    // -- 设置与同步状态 ------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()
            .map_err(CoreError::from)
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now],
        )?;
        Ok(())
    }

    pub fn get_sync_state(&self) -> Result<SyncClientState> {
        let row = self
            .conn
            .query_row(
                "SELECT cursor, generation, last_sync_at, last_result, last_error, clock_offset_ms
                 FROM sync_client_state WHERE id = 1",
                [],
                |r| {
                    Ok(SyncClientState {
                        cursor: r.get("cursor")?,
                        generation: r.get("generation")?,
                        last_sync_at: r.get("last_sync_at")?,
                        last_result: r.get("last_result")?,
                        last_error: r.get("last_error")?,
                        clock_offset_ms: r.get("clock_offset_ms")?,
                    })
                },
            )
            .optional()?;
        Ok(row.unwrap_or(SyncClientState {
            cursor: 0,
            generation: None,
            last_sync_at: None,
            last_result: None,
            last_error: None,
            clock_offset_ms: None,
        }))
    }

    pub fn save_sync_state(&mut self, state: &SyncClientState) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_client_state (id, cursor, generation, last_sync_at, last_result, last_error, clock_offset_ms)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET cursor = excluded.cursor,
               generation = excluded.generation,
               last_sync_at = excluded.last_sync_at, last_result = excluded.last_result,
               last_error = excluded.last_error, clock_offset_ms = excluded.clock_offset_ms",
            params![
                state.cursor,
                state.generation,
                state.last_sync_at,
                state.last_result,
                state.last_error,
                state.clock_offset_ms,
            ],
        )?;
        Ok(())
    }

    // -- 统一写入口（私有）：任何写操作都经这里打时间戳 / dirty / origin ----

    fn touch_list(&mut self, list: &TodoList) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "UPDATE lists SET name = ?1, color = ?2, sort_order = ?3, updated_at = ?4,
             dirty = 1, origin = ?5 WHERE id = ?6",
            params![
                list.name,
                list.color,
                list.sort_order,
                now,
                self.device_id,
                list.id
            ],
        )?;
        Ok(())
    }

    fn touch_task(&mut self, task: &Task) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "UPDATE tasks SET list_id = ?1, title = ?2, notes = ?3, status = ?4, priority = ?5,
             due_at = ?6, completed_at = ?7, sort_order = ?8, updated_at = ?9,
             dirty = 1, origin = ?10 WHERE id = ?11",
            params![
                task.list_id,
                task.title,
                task.notes,
                task.status,
                task.priority,
                task.due_at,
                task.completed_at,
                task.sort_order,
                now,
                self.device_id,
                task.id,
            ],
        )?;
        Ok(())
    }
}

/// 同步写入口用的事务内读写（只在 apply_sync 内用，不暴露给前端）。
fn get_list_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<Option<TodoList>> {
    Ok(tx
        .query_row(
            "SELECT id, name, color, sort_order, deleted_at, updated_at, origin, created_at
             FROM lists WHERE id = ?1",
            params![id],
            row_to_list,
        )
        .optional()?)
}

fn get_task_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<Option<Task>> {
    Ok(tx
        .query_row(
            "SELECT id, list_id, title, notes, status, priority, due_at, completed_at,
                    sort_order, deleted_at, updated_at, origin, created_at
             FROM tasks WHERE id = ?1",
            params![id],
            row_to_task,
        )
        .optional()?)
}

fn dirty_of_list_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<bool> {
    let dirty: i64 = tx.query_row("SELECT dirty FROM lists WHERE id = ?1", params![id], |r| {
        r.get(0)
    })?;
    Ok(dirty == 1)
}

fn dirty_of_task_tx(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<bool> {
    let dirty: i64 = tx.query_row("SELECT dirty FROM tasks WHERE id = ?1", params![id], |r| {
        r.get(0)
    })?;
    Ok(dirty == 1)
}

/// 服务端权威态原样写入并清 dirty：保留服务端 updated_at/origin，不打新戳。
fn upsert_list_tx(tx: &rusqlite::Transaction<'_>, list: &TodoList) -> Result<()> {
    tx.execute(
        "INSERT INTO lists (id, name, color, sort_order, deleted_at, dirty, updated_at, origin, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, color = excluded.color,
           sort_order = excluded.sort_order, deleted_at = excluded.deleted_at, dirty = 0,
           updated_at = excluded.updated_at, origin = excluded.origin, created_at = excluded.created_at",
        params![
            list.id,
            list.name,
            list.color,
            list.sort_order,
            list.deleted_at,
            list.updated_at,
            list.origin,
            list.created_at
        ],
    )?;
    Ok(())
}

fn upsert_task_tx(tx: &rusqlite::Transaction<'_>, task: &Task) -> Result<()> {
    tx.execute(
        "INSERT INTO tasks (id, list_id, title, notes, status, priority, due_at, completed_at,
                            sort_order, deleted_at, dirty, updated_at, origin, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET list_id = excluded.list_id, title = excluded.title,
           notes = excluded.notes, status = excluded.status, priority = excluded.priority,
           due_at = excluded.due_at, completed_at = excluded.completed_at,
           sort_order = excluded.sort_order, deleted_at = excluded.deleted_at, dirty = 0,
           updated_at = excluded.updated_at, origin = excluded.origin, created_at = excluded.created_at",
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
            task.created_at
        ],
    )?;
    Ok(())
}

fn load_or_create_device_id(conn: &Connection) -> Result<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![SETTING_DEVICE_ID],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        if !id.is_empty() {
            return Ok(id);
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![SETTING_DEVICE_ID, id, now_ms()],
    )?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn inbox_created_on_first_open() {
        let repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let lists = repo.list_lists().unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].id, INBOX_ID);
        assert_eq!(lists[0].name, INBOX_NAME);
        assert_eq!(repo.pending_dirty_count().unwrap(), 1); // 收件箱 dirty=1 待首次同步
    }

    #[test]
    fn writes_stamp_updated_at_dirty_and_origin() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let origin = repo.device_id().to_string();
        let task = repo.create_task(INBOX_ID, "写 SPEC", "", 2, None).unwrap();
        assert_eq!(task.origin, origin);
        assert!(task.updated_at > 0);
        assert_eq!(repo.pending_dirty_count().unwrap(), 2); // 收件箱 + 新任务

        let before = task.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let toggled = repo.toggle_task(&task.id).unwrap();
        assert_eq!(toggled.status, STATUS_DONE);
        assert!(toggled.completed_at.is_some());
        assert!(toggled.updated_at >= before);
        assert_eq!(toggled.origin, origin);
    }

    #[test]
    fn soft_delete_and_search() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let a = repo.create_task(INBOX_ID, "买牛奶", "", 0, None).unwrap();
        repo.create_task(INBOX_ID, "写报告", "", 0, None).unwrap();
        repo.delete_task(&a.id).unwrap();
        assert!(repo
            .list_tasks(INBOX_ID)
            .unwrap()
            .iter()
            .all(|t| t.id != a.id));
        assert_eq!(repo.search_tasks("报告", 10).unwrap().len(), 1);
        assert_eq!(repo.search_tasks("牛奶", 10).unwrap().len(), 0);
        // 软删的记录仍在 dirty 集里（tombstone 要同步出去）
        assert!(repo
            .dirty_changes()
            .unwrap()
            .tasks
            .iter()
            .any(|t| t.id == a.id));
    }

    #[test]
    fn hard_delete_forbidden_while_dirty() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let a = repo.create_task(INBOX_ID, "临时", "", 0, None).unwrap();
        repo.delete_task(&a.id).unwrap();
        assert!(repo.hard_delete_task(&a.id).is_err());
    }

    #[test]
    fn list_crud_and_counts() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let list = repo.create_list("工作", "#FF0000").unwrap();
        repo.create_task(&list.id, "开会", "", 0, None).unwrap();
        let counts = repo.list_counts().unwrap();
        assert_eq!(
            counts.iter().find(|c| c.list_id == list.id).unwrap().active,
            1
        );
        let renamed = repo.update_list(&list.id, Some("工作-改"), None).unwrap();
        assert_eq!(renamed.name, "工作-改");
        repo.delete_list(&list.id).unwrap();
        assert!(repo.list_lists().unwrap().iter().all(|l| l.id != list.id));
    }

    #[test]
    fn reorder_list_changes_order() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let _a = repo.create_list("甲", "#FF0000").unwrap();
        let b = repo.create_list("乙", "#00FF00").unwrap();
        // 乙移到最前（与收件箱同为 0 时按 created_at 排在收件箱后）
        repo.reorder_list(&b.id, 0).unwrap();
        let lists = repo.list_lists().unwrap();
        assert_eq!(lists[1].id, b.id);
        assert_eq!(lists[1].sort_order, 0);
        // 重排会打 dirty（要同步出去）
        assert!(repo
            .dirty_changes()
            .unwrap()
            .lists
            .iter()
            .any(|l| l.id == b.id));
    }

    fn sync_state(cursor: i64) -> SyncClientState {
        SyncClientState {
            cursor,
            generation: None,
            last_sync_at: Some(9999),
            last_result: Some("ok".to_string()),
            last_error: None,
            clock_offset_ms: Some(0),
        }
    }

    #[test]
    fn generation_mismatch_rules() {
        // 相同 → 不变化
        assert!(!generation_mismatch(
            &Some("g1".to_string()),
            &Some("g1".to_string()),
            5
        ));
        // 不同 → 变化，全量重同步
        assert!(generation_mismatch(
            &Some("g1".to_string()),
            &Some("g2".to_string()),
            5
        ));
        // 旧库无 generation、cursor>0 → 视为变化（第一次遇到新服务端即全量一次）
        assert!(generation_mismatch(&None, &Some("g2".to_string()), 5));
        // 首次同步（cursor=0）→ 不变化，只记录
        assert!(!generation_mismatch(&None, &Some("g2".to_string()), 0));
        // 服务端没给 generation → 不变化
        assert!(!generation_mismatch(&Some("g1".to_string()), &None, 5));
        assert!(!generation_mismatch(&None, &None, 5));
    }

    #[test]
    fn apply_sync_clears_pushed_dirty_and_applies_server_state() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let task = repo.create_task(INBOX_ID, "本地标题", "", 0, None).unwrap();
        let pushed = SyncChanges {
            lists: vec![],
            tasks: vec![task.clone()],
        };
        // 服务端接受后返回同一条（权威态与推送一致），dirty 清掉
        let apply = SyncChanges {
            lists: vec![],
            tasks: vec![task.clone()],
        };
        repo.apply_sync(&pushed, &apply, 0, &sync_state(10))
            .unwrap();
        assert_eq!(repo.pending_dirty_count().unwrap(), 1); // 只剩收件箱
        assert_eq!(repo.get_sync_state().unwrap().cursor, 10);
        assert_eq!(repo.get_task(&task.id).unwrap().title, "本地标题");
    }

    #[test]
    fn apply_sync_keeps_local_newer_dirty_change() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let task = repo.create_task(INBOX_ID, "旧标题", "", 0, None).unwrap();
        let pushed = SyncChanges {
            lists: vec![],
            tasks: vec![task.clone()],
        };
        // 同步往返期间本地又改了一次：updated_at 更大，apply 里的旧服务端版不能覆盖
        std::thread::sleep(std::time::Duration::from_millis(2));
        repo.update_task(
            &task.id,
            TaskPatch {
                title: Some("新标题".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let newer = repo.get_task(&task.id).unwrap();
        assert!(newer.updated_at > task.updated_at);
        let server_old = Task {
            title: "服务端旧标题".to_string(),
            ..task.clone()
        };
        let apply = SyncChanges {
            lists: vec![],
            tasks: vec![server_old],
        };
        repo.apply_sync(&pushed, &apply, 0, &sync_state(10))
            .unwrap();
        // 推送时的旧值没清 dirty（本地更新），apply 也没覆盖
        assert_eq!(repo.get_task(&task.id).unwrap().title, "新标题");
        assert!(repo
            .dirty_changes()
            .unwrap()
            .tasks
            .iter()
            .any(|t| t.id == task.id));
    }

    #[test]
    fn apply_sync_propagates_server_tombstone_and_skips_orphan_task() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let task = repo.create_task(INBOX_ID, "待删", "", 0, None).unwrap();
        let tomb = Task {
            deleted_at: Some(task.updated_at + 1000),
            updated_at: task.updated_at + 1000,
            origin: "other-device".to_string(),
            ..task.clone()
        };
        let apply = SyncChanges {
            lists: vec![],
            tasks: vec![
                tomb,
                // 列表不存在的孤儿任务应跳过，不报错
                Task {
                    id: "orphan".to_string(),
                    list_id: "missing-list".to_string(),
                    title: "孤儿".to_string(),
                    ..task.clone()
                },
            ],
        };
        let outcome = repo
            .apply_sync(&SyncChanges::default(), &apply, 2, &sync_state(5))
            .unwrap();
        // v4.4：孤儿任务被跳过时 cursor 不得推进（保持同步前的值 2，而不是响应值 5），
        // 状态记 error 而不是 ok——M12 的静默丢数据就是从这里漏过去的。
        assert_eq!(outcome.skipped_tasks, 1);
        assert_eq!(outcome.skipped_task_ids, vec!["orphan".to_string()]);
        let saved = repo.get_sync_state().unwrap();
        assert_eq!(saved.cursor, 2);
        assert_eq!(saved.last_result.as_deref(), Some("error"));
        assert!(saved
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains('1'));
        assert!(repo
            .list_tasks(INBOX_ID)
            .unwrap()
            .iter()
            .all(|t| t.id != task.id));
        assert!(repo.get_task("orphan").is_err());
    }

    #[test]
    fn apply_sync_stores_generation() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let mut state = sync_state(10);
        state.generation = Some("g9".to_string());
        repo.apply_sync(&SyncChanges::default(), &SyncChanges::default(), 0, &state)
            .unwrap();
        assert_eq!(
            repo.get_sync_state().unwrap().generation.as_deref(),
            Some("g9")
        );
    }

    #[test]
    fn reset_sync_state_clears_cursor_and_generation_and_marks_all_dirty() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let task = repo.create_task(INBOX_ID, "未同步的", "", 0, None).unwrap();
        let mut state = sync_state(7);
        state.generation = Some("g1".to_string());
        repo.save_sync_state(&state).unwrap();
        assert_eq!(repo.get_sync_state().unwrap().cursor, 7);
        // 先把这条记为已同步（干净），再重置：重置必须让它重新变 dirty，
        // 否则服务端换库后这条干净记录永远推不回新库。
        repo.apply_sync(
            &SyncChanges {
                lists: vec![],
                tasks: vec![task.clone()],
            },
            &SyncChanges::default(),
            0,
            &sync_state(7),
        )
        .unwrap();
        assert_eq!(repo.pending_dirty_count().unwrap(), 1); // 只剩收件箱
        repo.reset_sync_state().unwrap();
        let saved = repo.get_sync_state().unwrap();
        assert_eq!(saved.cursor, 0);
        assert_eq!(saved.generation, None);
        // 本地记录（含刚才干净的那条）全部重新变 dirty，下次同步全量重推
        assert!(repo
            .dirty_changes()
            .unwrap()
            .tasks
            .iter()
            .any(|t| t.id == task.id));
    }

    #[test]
    fn mark_all_dirty_repushes_clean_records() {
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let task = repo.create_task(INBOX_ID, "已同步的", "", 0, None).unwrap();
        repo.apply_sync(
            &SyncChanges {
                lists: vec![],
                tasks: vec![task.clone()],
            },
            &SyncChanges::default(),
            0,
            &sync_state(0),
        )
        .unwrap();
        assert!(repo.dirty_changes().unwrap().tasks.is_empty());
        repo.mark_all_dirty().unwrap();
        assert!(repo
            .dirty_changes()
            .unwrap()
            .tasks
            .iter()
            .any(|t| t.id == task.id));
    }

    #[test]
    fn apply_sync_rejected_record_converges_only_when_server_version_in_apply() {
        // §9.3 / v4.7：客户端已拉过胜出版本，之后离线用偏慢时钟改同一条，
        // 推上去被拒。若服务端不把权威版本放进 apply，本地清 dirty 后仍留旧内容、
        // 后续同步不再收敛（与 cursor 丢数据同类的静默留旧）。
        let mut repo = Repo::new(open_in_memory().unwrap()).unwrap();
        let task = repo.create_task(INBOX_ID, "Y", "", 0, None).unwrap();
        // 离线偏慢时钟改动：标题 X-old、updated_at 更旧，dirty=1。
        let stale_at = task.updated_at - 60_000;
        repo.conn
            .execute(
                "UPDATE tasks SET title = 'X-old', updated_at = ?1, dirty = 1 WHERE id = ?2",
                params![stale_at, task.id],
            )
            .unwrap();
        let stale = Task {
            title: "X-old".to_string(),
            updated_at: stale_at,
            ..task.clone()
        };
        // 第 1 轮：服务端只回 conflicts、不下发权威版本（未修复行为）——
        // 本地清 dirty 却留着 X-old，且 dirty 已清、后续不再重推。
        let pushed = SyncChanges {
            lists: vec![],
            tasks: vec![stale],
        };
        repo.apply_sync(&pushed, &SyncChanges::default(), 2, &sync_state(2))
            .unwrap();
        assert_eq!(repo.get_task(&task.id).unwrap().title, "X-old");
        assert!(!repo
            .dirty_changes()
            .unwrap()
            .tasks
            .iter()
            .any(|t| t.id == task.id));
        // 第 2 轮：服务端把被拒记录的权威版本放进 apply（v4.7 方案 A）——
        // 本地 updated_at 更旧，步骤 4 按 updated_at 比对覆盖，收敛到 Y。
        let apply = SyncChanges {
            lists: vec![],
            tasks: vec![task.clone()],
        };
        repo.apply_sync(&SyncChanges::default(), &apply, 2, &sync_state(2))
            .unwrap();
        assert_eq!(repo.get_task(&task.id).unwrap().title, "Y");
    }
}
