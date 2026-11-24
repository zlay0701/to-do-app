//! Tauri IPC 命令（SPEC §7）。前端只调这些命令，不直接碰数据库。
//! 令牌只写不读：`get_settings` 只返回 `hasToken`。

use crate::secret::SecretStore;
use crate::sync_client::SyncError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;
use todo_core::db::repo::{ListCount, Repo, TaskPatch};
use todo_core::models::{SyncChanges, SyncClientState, SyncRequest, Task, TodoList};
use todo_core::now_ms;

type CmdResult<T> = Result<T, String>;

fn lock_repo<'a>(state: &'a State<'_, AppState>) -> CmdResult<std::sync::MutexGuard<'a, Repo>> {
    state.repo.lock().map_err(|e| format!("数据库被锁：{e}"))
}

// -- 列表 -------------------------------------------------------------------

#[tauri::command]
pub fn list_lists(state: State<'_, AppState>) -> CmdResult<Vec<TodoList>> {
    lock_repo(&state)?.list_lists().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_list(state: State<'_, AppState>, name: String, color: String) -> CmdResult<TodoList> {
    lock_repo(&state)?
        .create_list(&name, &color)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_list(state: State<'_, AppState>, id: String, name: String) -> CmdResult<TodoList> {
    lock_repo(&state)?
        .update_list(&id, Some(&name), None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_list(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    lock_repo(&state)?
        .delete_list(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_list(state: State<'_, AppState>, id: String, sort_order: i64) -> CmdResult<()> {
    lock_repo(&state)?
        .reorder_list(&id, sort_order)
        .map_err(|e| e.to_string())
}

// -- 任务 -------------------------------------------------------------------

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>, list_id: String) -> CmdResult<Vec<Task>> {
    lock_repo(&state)?
        .list_tasks(&list_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_task(
    state: State<'_, AppState>,
    list_id: String,
    title: String,
    notes: String,
    priority: i32,
    due_at: Option<i64>,
) -> CmdResult<Task> {
    lock_repo(&state)?
        .create_task(&list_id, &title, &notes, priority, due_at)
        .map_err(|e| e.to_string())
}

/// `dueAt: null` 表示“清空到期时间”。serde 对 `Option<Option<T>>` 会把
/// JSON null 解成外层 None（与字段未提供同义），导致 update_task 里的
/// `if let Some(due_at)` 不成立、旧 dueAt 被保留。这里显式把 null 映射成
/// `Some(None)`，而字段缺省仍由 `#[serde(default)]` 得到 `None`。
fn deserialize_due_at_patch<'de, D>(deserializer: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <Option<i64> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(Some(value))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskInput {
    title: Option<String>,
    notes: Option<String>,
    priority: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_due_at_patch")]
    due_at: Option<Option<i64>>,
    list_id: Option<String>,
}

#[tauri::command]
pub fn update_task(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateTaskInput,
) -> CmdResult<Task> {
    lock_repo(&state)?
        .update_task(
            &id,
            TaskPatch {
                title: patch.title,
                notes: patch.notes,
                priority: patch.priority,
                due_at: patch.due_at,
                list_id: patch.list_id,
            },
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_task(state: State<'_, AppState>, id: String) -> CmdResult<Task> {
    lock_repo(&state)?
        .toggle_task(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    lock_repo(&state)?
        .delete_task(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_task(state: State<'_, AppState>, id: String, sort_order: f64) -> CmdResult<()> {
    lock_repo(&state)?
        .reorder_task(&id, sort_order)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_tasks(state: State<'_, AppState>, query: String) -> CmdResult<Vec<Task>> {
    lock_repo(&state)?
        .search_tasks(&query, 50)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountsOut {
    counts: Vec<ListCount>,
    pending_dirty: i64,
}

#[tauri::command]
pub fn counts(state: State<'_, AppState>) -> CmdResult<CountsOut> {
    let repo = lock_repo(&state)?;
    Ok(CountsOut {
        counts: repo.list_counts().map_err(|e| e.to_string())?,
        pending_dirty: repo.pending_dirty_count().map_err(|e| e.to_string())?,
    })
}

// -- 设置 -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettings {
    #[serde(default)]
    pub server_url: String,
    #[serde(default = "default_sync_interval")]
    pub sync_interval_min: i64,
    #[serde(default = "default_true")]
    pub auto_sync: bool,
}

fn default_sync_interval() -> i64 {
    15
}

fn default_true() -> bool {
    true
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            sync_interval_min: 15,
            auto_sync: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalNotifySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub before_min: i64,
}

impl Default for LocalNotifySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            before_min: 0,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    server: ServerSettings,
    local_notify: LocalNotifySettings,
    has_token: bool,
}

fn read_server_settings(repo: &Repo) -> ServerSettings {
    let mut s = ServerSettings::default();
    if let Ok(Some(v)) = repo.get_setting("server.url") {
        s.server_url = v;
    }
    if let Ok(Some(v)) = repo.get_setting("server.sync_interval_min") {
        if let Ok(n) = v.parse() {
            s.sync_interval_min = n;
        }
    }
    if let Ok(Some(v)) = repo.get_setting("server.auto_sync") {
        s.auto_sync = v != "0" && v != "false";
    }
    s
}

fn read_notify_settings(repo: &Repo) -> LocalNotifySettings {
    let mut s = LocalNotifySettings::default();
    if let Ok(Some(v)) = repo.get_setting("notify.enabled") {
        s.enabled = v != "0" && v != "false";
    }
    if let Ok(Some(v)) = repo.get_setting("notify.before_min") {
        if let Ok(n) = v.parse() {
            s.before_min = n;
        }
    }
    s
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    let repo = lock_repo(&state)?;
    Ok(AppSettings {
        server: read_server_settings(&repo),
        local_notify: read_notify_settings(&repo),
        has_token: state.secrets.has_token(),
    })
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    scope: String,
    payload: serde_json::Value,
) -> CmdResult<()> {
    // 令牌单独处理：只进凭据库，不进 settings 表、不回显。
    if scope == "server" {
        if let Some(token) = payload.get("token").and_then(|v| v.as_str()) {
            if token.is_empty() {
                state.secrets.delete_token()?;
            } else {
                state.secrets.set_token(token)?;
            }
        }
        let mut repo = lock_repo(&state)?;
        if let Some(url) = payload.get("serverUrl").and_then(|v| v.as_str()) {
            repo.set_setting("server.url", url)
                .map_err(|e| e.to_string())?;
        }
        if let Some(n) = payload.get("syncIntervalMin").and_then(|v| v.as_i64()) {
            repo.set_setting("server.sync_interval_min", &n.to_string())
                .map_err(|e| e.to_string())?;
        }
        if let Some(b) = payload.get("autoSync").and_then(|v| v.as_bool()) {
            repo.set_setting("server.auto_sync", if b { "1" } else { "0" })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    } else if scope == "local_notify" {
        let mut repo = lock_repo(&state)?;
        if let Some(b) = payload.get("enabled").and_then(|v| v.as_bool()) {
            repo.set_setting("notify.enabled", if b { "1" } else { "0" })
                .map_err(|e| e.to_string())?;
        }
        if let Some(n) = payload.get("beforeMin").and_then(|v| v.as_i64()) {
            repo.set_setting("notify.before_min", &n.to_string())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    } else {
        Err(format!("未知的设置分区：{scope}"))
    }
}

// -- 服务端连通性 ------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    ok: bool,
    status: Option<u16>,
    message: String,
}

#[tauri::command]
pub async fn test_server(state: State<'_, AppState>) -> CmdResult<TestResult> {
    let (url, token) = {
        let repo = lock_repo(&state)?;
        let url = read_server_settings(&repo).server_url;
        let token = state.secrets.get_token()?.unwrap_or_default();
        (url, token)
    };
    if url.trim().is_empty() {
        return Ok(TestResult {
            ok: false,
            status: None,
            message: "请先填写服务端地址".to_string(),
        });
    }
    let ping_url = format!("{}/api/v1/ping", url.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(TestResult {
                ok: false,
                status: None,
                message: format!("无法创建请求：{e}"),
            });
        }
    };
    let resp = client.get(&ping_url).bearer_auth(&token).send().await;
    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            if r.status().is_success() {
                Ok(TestResult {
                    ok: true,
                    status: Some(status),
                    message: "连接成功".to_string(),
                })
            } else if status == 401 {
                Ok(TestResult {
                    ok: false,
                    status: Some(status),
                    message: "令牌无效，请重新配置".to_string(),
                })
            } else {
                Ok(TestResult {
                    ok: false,
                    status: Some(status),
                    message: format!("服务端返回 HTTP {status}"),
                })
            }
        }
        Err(e) => Ok(TestResult {
            ok: false,
            status: None,
            message: if e.is_connect() || e.is_timeout() {
                "无法连接服务端，请检查地址与网络".to_string()
            } else {
                format!("请求失败：{e}")
            },
        }),
    }
}

// -- 同步状态（M5 之前为占位） ------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusOut {
    state: String,
    last_sync_at: Option<i64>,
    pending_count: i64,
    last_error: Option<String>,
    clock_offset_ms: Option<i64>,
}

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> CmdResult<SyncStatusOut> {
    let repo = lock_repo(&state)?;
    let saved: SyncClientState = repo.get_sync_state().map_err(|e| e.to_string())?;
    let pending = repo.pending_dirty_count().map_err(|e| e.to_string())?;
    let sync_state = if state.sync.is_busy() {
        "syncing".to_string()
    } else if saved.last_result.as_deref() == Some("error") {
        "error".to_string()
    } else if saved.last_result.as_deref() == Some("conflict") {
        "conflict".to_string()
    } else if saved.last_sync_at.is_some() {
        "ok".to_string()
    } else {
        "idle".to_string()
    };
    Ok(SyncStatusOut {
        state: sync_state,
        last_sync_at: saved.last_sync_at,
        pending_count: pending,
        last_error: saved.last_error,
        clock_offset_ms: saved.clock_offset_ms,
    })
}

#[tauri::command]
pub async fn sync_now(state: State<'_, AppState>) -> CmdResult<String> {
    if !state.sync.try_begin() {
        return Err("BUSY".to_string());
    }
    let result = do_sync(&state).await;
    state.sync.finish();
    result
}

/// 重置同步（v4.4，§9.5 兜底）：cursor 与 generation 清零、保留本地 dirty，
/// 下次同步全量重拉。设置页确认后调用。
#[tauri::command]
pub fn reset_sync(state: State<'_, AppState>) -> CmdResult<String> {
    let mut repo = lock_repo(&state)?;
    repo.reset_sync_state().map_err(|e| e.to_string())?;
    Ok("已重置同步：下次同步将全量拉取服务端数据，本地全部记录会重新推送".to_string())
}

/// 一次同步往返（§9.3）：收集 dirty → POST → 单事务应用 → 写 cursor。
/// apply 单次最多 5000 条，满 5000 时用空 changes 循环拉取直到拉空。
/// v4.4：响应 generation 与本地不一致时按首次同步处理——丢弃本轮响应、
/// cursor 归零后重推 dirty、全量拉取（§9.5）；只触发一次。
async fn do_sync(state: &State<'_, AppState>) -> CmdResult<String> {
    let (url, token, device_id, cursor, saved_generation, mut dirty) = {
        let repo = lock_repo(state)?;
        let url = read_server_settings(&repo).server_url;
        let token = state.secrets.get_token()?.unwrap_or_default();
        let device_id = repo.device_id().to_string();
        let saved_state = repo.get_sync_state().map_err(|e| e.to_string())?;
        let dirty = repo.dirty_changes().map_err(|e| e.to_string())?;
        (
            url,
            token,
            device_id,
            saved_state.cursor,
            saved_state.generation,
            dirty,
        )
    };
    eprintln!(
        "[todo] sync_now: url_configured={} cred={} dirty={} cursor={}",
        !url.trim().is_empty(),
        if token.is_empty() { 0 } else { 1 },
        dirty.lists.len() + dirty.tasks.len(),
        cursor
    );
    if url.trim().is_empty() {
        return Err("未配置服务端地址，请先在设置页填写".to_string());
    }
    if token.is_empty() {
        return Err("未配置同步令牌，请先在设置页填写".to_string());
    }
    let platform = match std::env::consts::OS {
        "windows" => "windows",
        "android" => "android",
        "macos" => "macos",
        _ => "linux",
    };
    let device_name = format!("{platform}-{}", &device_id[..8.min(device_id.len())]);

    let mut cursor = cursor;
    let mut saved_generation = saved_generation;
    let mut total_conflicts: i64 = 0;
    let mut applied: usize = 0;
    let mut generation_reset = false;
    let mut need_repush = false;
    for round in 0..20 {
        let round_cursor = cursor;
        let changes = if round == 0 || need_repush {
            need_repush = false;
            dirty.clone()
        } else {
            SyncChanges::default()
        };
        let request = SyncRequest {
            device_id: device_id.clone(),
            device_name: device_name.clone(),
            platform: platform.to_string(),
            last_cursor: cursor,
            client_time: now_ms(),
            changes: changes.clone(),
        };
        let resp = match state.sync.post_sync(&url, &token, &request).await {
            Ok(resp) => {
                eprintln!(
                    "[todo] sync round {round}: ok cursor={} conflicts={} apply={}",
                    resp.cursor,
                    resp.conflicts,
                    resp.apply.lists.len() + resp.apply.tasks.len()
                );
                resp
            }
            Err(SyncError::Unauthorized) => {
                // 401：停止自动同步，状态区红色"令牌无效"（§12）。
                let mut repo = lock_repo(state)?;
                let _ = repo.set_setting("server.auto_sync", "0");
                let mut saved = repo.get_sync_state().map_err(|e| e.to_string())?;
                saved.last_result = Some("error".to_string());
                saved.last_error = Some("令牌无效，请重新配置".to_string());
                repo.save_sync_state(&saved).map_err(|e| e.to_string())?;
                return Err("令牌无效，请重新配置".to_string());
            }
            Err(SyncError::TooFrequent) => {
                // 429：静默退避，不记错误状态。
                return Ok("同步过于频繁，稍后重试".to_string());
            }
            Err(e) => {
                // 网络/超时/其他：不调 apply_sync，dirty 原样保留，下轮重试。
                let mut repo = lock_repo(state)?;
                let mut saved = repo.get_sync_state().map_err(|e| e.to_string())?;
                saved.last_result = Some("error".to_string());
                saved.last_error = Some(e.to_string());
                repo.save_sync_state(&saved).map_err(|e| e.to_string())?;
                return Err(e.to_string());
            }
        };
        // v4.4：服务端换库（generation 变化）→ 按首次同步处理。
        // 本轮响应直接丢弃：还没调 apply_sync，本地 dirty 原样保留；
        // 先把 cursor=0 与新 generation 落库（中途崩溃也不丢重置决定），
        // 再重推 dirty、全量拉取。只触发一次；这不是错误，状态区不报红。
        if !generation_reset
            && todo_core::db::repo::generation_mismatch(
                &saved_generation,
                &resp.generation,
                round_cursor,
            )
        {
            generation_reset = true;
            {
                let mut repo = lock_repo(state)?;
                let mut saved = repo.get_sync_state().map_err(|e| e.to_string())?;
                saved.cursor = 0;
                saved.generation = resp.generation.clone();
                saved.last_result = None;
                saved.last_error = None;
                repo.save_sync_state(&saved).map_err(|e| e.to_string())?;
                // v4.4：按首次同步处理——本地全部记录重新标记 dirty 再重推。
                // 只重推原 dirty 会让新库永久缺失本地已同步过的干净记录（第二台设备
                // 也拉不到）；全部重推后 LWW 自然收敛，之后恢复增量。
                repo.mark_all_dirty().map_err(|e| e.to_string())?;
                dirty = repo.dirty_changes().map_err(|e| e.to_string())?;
            }
            saved_generation = resp.generation.clone();
            cursor = 0;
            need_repush = true;
            // 刚同步过一次，服务端限频 5 秒：等一下再重发，避免 429 空跑。
            // （万一重发仍被限频，429 分支直接返回——cursor=0 已落库，下次同步全量重拉。）
            // 用 spawn_blocking 睡眠，不占 async 执行器线程（客户端 crate 不直接依赖 tokio）。
            tauri::async_runtime::spawn_blocking(|| {
                std::thread::sleep(std::time::Duration::from_millis(5500));
            })
            .await
            .map_err(|e| e.to_string())?;
            eprintln!("[todo] sync: generation 变化，服务端已重建，cursor 归零全量重同步");
            continue;
        }
        total_conflicts += resp.conflicts;
        applied += resp.apply.lists.len() + resp.apply.tasks.len();
        let apply_count = resp.apply.lists.len() + resp.apply.tasks.len();
        let new_state = SyncClientState {
            cursor: resp.cursor,
            generation: resp.generation.clone(),
            last_sync_at: Some(now_ms()),
            last_result: Some(if resp.conflicts > 0 {
                "conflict".to_string()
            } else {
                "ok".to_string()
            }),
            last_error: None,
            clock_offset_ms: Some(resp.clock_offset_ms),
        };
        let outcome = {
            let mut repo = lock_repo(state)?;
            repo.apply_sync(&changes, &resp.apply, round_cursor, &new_state)
                .map_err(|e| e.to_string())?
        };
        if outcome.skipped_tasks > 0 {
            // v4.4：有孤儿任务被跳过，cursor 已由 apply_sync 保持在同步前的值，
            // 状态已记 error。本轮不再循环拉取，下轮从旧 cursor 重拉；
            // 若服务端换库导致列表永不到，下一轮的 generation 检查会触发全量重同步。
            return Ok(format!(
                "同步完成但有 {} 条任务因所属列表未同步而暂存，已保留游标下次重试",
                outcome.skipped_tasks
            ));
        }
        cursor = resp.cursor;
        if apply_count < 5000 {
            break;
        }
    }
    if total_conflicts > 0 {
        Ok(format!(
            "同步完成：拉取 {applied} 条，{total_conflicts} 条冲突被服务端拒绝（以新者为准）"
        ))
    } else {
        Ok(format!("同步完成：拉取 {applied} 条"))
    }
}

// -- 本地通知 ---------------------------------------------------------------

#[tauri::command]
pub fn due_notifications(state: State<'_, AppState>) -> CmdResult<Vec<Task>> {
    let repo = lock_repo(&state)?;
    let settings = read_notify_settings(&repo);
    if !settings.enabled {
        return Ok(Vec::new());
    }
    Ok(crate::local_notify::due_notifications(
        &repo,
        settings.before_min,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_task_due_at_patch_distinguishes_null_from_absent() {
        let patch: UpdateTaskInput = serde_json::from_str(r#"{"dueAt":null}"#).unwrap();
        assert_eq!(patch.due_at, Some(None), "null 必须表示清空 dueAt");
        let patch: UpdateTaskInput = serde_json::from_str(r#"{"dueAt":123}"#).unwrap();
        assert_eq!(patch.due_at, Some(Some(123)));
        let patch: UpdateTaskInput = serde_json::from_str("{}").unwrap();
        assert_eq!(patch.due_at, None, "字段缺省不能改动 dueAt");
    }
}
