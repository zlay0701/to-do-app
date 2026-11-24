//! 服务端共享状态：单写者 SQLite 连接（Mutex）、主密钥、HTTP 客户端。

use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub master_key: [u8; 32],
    pub http: reqwest::Client,
    pub poll_interval_secs: u64,
}

impl AppState {
    pub fn new(db: Connection, master_key: [u8; 32], poll_interval_secs: u64) -> Arc<Self> {
        Arc::new(Self {
            db: Mutex::new(db),
            master_key,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            poll_interval_secs,
        })
    }
}
