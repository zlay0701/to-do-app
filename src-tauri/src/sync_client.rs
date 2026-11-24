//! 客户端同步引擎（SPEC §5.1、§9.3，M5）。
//!
//! 只负责一件事：把 `SyncRequest` POST 到服务端并按 HTTP 状态分类错误。
//! 本地应用（dirty 清除 / apply 覆盖 / 写 cursor）走 `core::Repo::apply_sync`
//! 的统一写入口，由 commands.rs 在拿到响应后调用；超时/失败时根本不调它，
//! 所以 dirty 不会被误清（§12：超时整个事务回滚）。

use std::sync::atomic::{AtomicBool, Ordering};
use todo_core::models::{SyncRequest, SyncResponse};

pub struct SyncClient {
    busy: AtomicBool,
    http: reqwest::Client,
}

#[derive(Debug)]
pub enum SyncError {
    /// 401：令牌无效，停止自动同步（§12）。
    Unauthorized,
    /// 429：静默退避到下个周期。
    TooFrequent,
    /// 网络 / 超时（30s）等。
    Network(String),
    /// 其他 HTTP 状态。
    Http(u16, String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Unauthorized => write!(f, "令牌无效，请重新配置"),
            SyncError::TooFrequent => write!(f, "同步过于频繁，稍后重试"),
            SyncError::Network(e) => write!(f, "无法连接服务端：{e}"),
            SyncError::Http(status, body) => write!(f, "服务端返回 HTTP {status}：{body}"),
        }
    }
}

impl SyncClient {
    pub fn new() -> Self {
        Self {
            busy: AtomicBool::new(false),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// 尝试取得同步锁；已在同步中返回 false（命令层据此返回 BUSY）。
    pub fn try_begin(&self) -> bool {
        self.busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn finish(&self) {
        self.busy.store(false, Ordering::SeqCst);
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub async fn post_sync(
        &self,
        server_url: &str,
        token: &str,
        request: &SyncRequest,
    ) -> Result<SyncResponse, SyncError> {
        let url = format!("{}/api/v1/sync", server_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(request)
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;
        let status = resp.status();
        if status.is_success() {
            resp.json::<SyncResponse>()
                .await
                .map_err(|e| SyncError::Network(format!("响应解析失败：{e}")))
        } else if status.as_u16() == 401 {
            Err(SyncError::Unauthorized)
        } else if status.as_u16() == 429 {
            Err(SyncError::TooFrequent)
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(SyncError::Http(status.as_u16(), body))
        }
    }
}

impl Default for SyncClient {
    fn default() -> Self {
        Self::new()
    }
}
