//! 服务端入口（SPEC §14）：`cargo build --release -p todo-server`，
//! `TODO_MASTER_KEY=… TODO_ADMIN_PASSWORD=… ./todo-server`。
//!
//! 启动条件：`TODO_MASTER_KEY`（32 字节 base64）缺失拒启动；
//! 裸 HTTP 须 `TODO_INSECURE_HTTP=1`（否则拒启动，生产应走 TLS 反代）。

mod api;
mod crypto;
mod db;
mod jobs;
mod state;

use crypto::{decode_master_key, encrypt};
use db::repo;
use state::AppState;
use std::path::PathBuf;
use todo_core::now_ms;

/// 读当前目录的 `.env`（若存在）：简单 `KEY=VALUE` 解析，不打印值。
/// 已设置的环境变量优先，不覆盖。
fn load_dotenv() {
    let Ok(content) = std::fs::read_to_string(".env") else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !key.is_empty() && std::env::var_os(key).is_none() {
                std::env::set_var(key, value);
            }
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() {
    load_dotenv();

    let master_key_encoded = match std::env::var("TODO_MASTER_KEY") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("TODO_MASTER_KEY 未设置（须为 32 字节 base64），拒绝启动");
            std::process::exit(1);
        }
    };
    let master_key = match decode_master_key(&master_key_encoded) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("TODO_MASTER_KEY 无效：{e}，拒绝启动");
            std::process::exit(1);
        }
    };
    if std::env::var("TODO_INSECURE_HTTP").unwrap_or_default() != "1" {
        eprintln!("未设置 TODO_INSECURE_HTTP=1：裸 HTTP 不允许，拒绝启动（生产请走 TLS 反代）");
        std::process::exit(1);
    }
    eprintln!("[todo-server] 警告：以裸 HTTP 运行（TODO_INSECURE_HTTP=1），生产请走 TLS 反代");

    let port: u16 = env_or("TODO_PORT", "8787").parse().unwrap_or(8787);
    let db_path = PathBuf::from(env_or("TODO_DB_PATH", "/data/app.db"));
    let poll_interval: u64 = env_or("TODO_POLL_INTERVAL", "60").parse().unwrap_or(60);

    let conn = match db::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("打开数据库失败：{e}");
            std::process::exit(1);
        }
    };

    // 首次启动种子：Admin 账号与同步令牌（只在库里没有时写，之后以库为准）。
    let now = now_ms();
    if let Ok(None) = repo::get_admin(&conn) {
        if let Ok(password) = std::env::var("TODO_ADMIN_PASSWORD") {
            if !password.is_empty() {
                match api::auth::hash_password(&password) {
                    Ok(hash) => {
                        let _ = repo::set_admin(&conn, "admin", &hash, now);
                        eprintln!("[todo-server] Admin 账号已初始化（admin）");
                    }
                    Err((status, _)) => {
                        eprintln!("[todo-server] Admin 密码哈希失败：HTTP {status}")
                    }
                }
            }
        }
    }
    if let Ok(None) = repo::get_secret(&conn, "sync_token") {
        if let Ok(token) = std::env::var("TODO_SYNC_TOKEN") {
            if !token.is_empty() {
                if let Ok(enc) = encrypt(&master_key, token.as_bytes()) {
                    let _ = repo::set_secret(&conn, "sync_token", &enc);
                    eprintln!("[todo-server] 同步令牌已写入（加密存储）");
                }
            }
        }
    }

    let state = AppState::new(conn, master_key, poll_interval);
    let scheduler_state = state.clone();
    tokio::spawn(async move {
        jobs::scheduler::scheduler_loop(scheduler_state).await;
    });

    let app = api::build_router(state);
    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("监听端口 {port} 失败：{e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "[todo-server] listening on 0.0.0.0:{port} (db: {})",
        db_path.display()
    );
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("服务异常退出：{e}");
        std::process::exit(1);
    }
}
