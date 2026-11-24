#[cfg(target_os = "android")]
mod android_secret;
mod commands;
mod local_notify;
mod secret;
mod sync_client;

use secret::SecretStoreImpl;
use std::sync::Mutex;
use sync_client::SyncClient;
use tauri::Manager;
use todo_core::db::repo::Repo;

pub struct AppState {
    pub repo: Mutex<Repo>,
    pub secrets: SecretStoreImpl,
    pub sync: SyncClient,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("无法确定应用数据目录：{e}"))?;
            std::fs::create_dir_all(&data_dir).map_err(|e| format!("无法创建应用数据目录：{e}"))?;
            let db_path = data_dir.join("todo.db");
            let conn = todo_core::db::open(&db_path).map_err(|e| format!("无法打开数据库：{e}"))?;
            let repo = Repo::new(conn).map_err(|e| format!("无法初始化数据层：{e}"))?;
            app.manage(AppState {
                repo: Mutex::new(repo),
                secrets: SecretStoreImpl::new(data_dir),
                sync: SyncClient::new(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_lists,
            commands::create_list,
            commands::rename_list,
            commands::delete_list,
            commands::reorder_list,
            commands::list_tasks,
            commands::create_task,
            commands::update_task,
            commands::toggle_task,
            commands::delete_task,
            commands::reorder_task,
            commands::search_tasks,
            commands::counts,
            commands::get_settings,
            commands::save_settings,
            commands::test_server,
            commands::sync_now,
            commands::sync_status,
            commands::reset_sync,
            commands::due_notifications,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("[todo] 应用启动失败：{e}");
            std::process::exit(1);
        });
}
