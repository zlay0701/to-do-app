//! 通知配置与测试发送（SPEC §8、§10）：GET/PUT /api/notify、POST /api/test/*。
//!
//! 密码与 Webhook 密钥不进配置 JSON，只进 `secrets` 表（AES-256-GCM 加密）；
//! GET 只返回 `hasSmtpPassword` / `hasWebhookSecret` 两个布尔值。

use crate::api::auth::{err, require_admin, ApiResult};
use crate::crypto::{decrypt, encrypt};
use crate::db::repo;
use crate::jobs::dispatcher;
use crate::state::AppState;
use axum::http::{HeaderMap, StatusCode};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default)]
    pub smtp_user: String,
    #[serde(default)]
    pub mail_from: String,
    #[serde(default)]
    pub mail_to: Vec<String>,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default = "default_offsets")]
    pub remind_offsets: Vec<i64>,
    #[serde(default = "default_catch_up")]
    pub catch_up_min: i64,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub digest_enabled: bool,
    #[serde(default = "default_digest_hour")]
    pub digest_hour: u8,
}

fn default_true() -> bool {
    true
}
fn default_smtp_port() -> u16 {
    587
}
fn default_offsets() -> Vec<i64> {
    vec![0]
}
fn default_catch_up() -> i64 {
    120
}
fn default_timezone() -> String {
    "Asia/Shanghai".to_string()
}
fn default_digest_hour() -> u8 {
    8
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            smtp_host: String::new(),
            smtp_port: 587,
            smtp_user: String::new(),
            mail_from: String::new(),
            mail_to: Vec::new(),
            webhook_url: String::new(),
            remind_offsets: vec![0],
            catch_up_min: 120,
            timezone: "Asia/Shanghai".to_string(),
            digest_enabled: false,
            digest_hour: 8,
        }
    }
}

impl NotifyConfig {
    /// 校验用户填的值：时区必须能解析，偏移与补发窗口不能为负。
    pub fn validate(&self) -> Result<(), String> {
        if self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(format!("未知时区：{}", self.timezone));
        }
        if self.remind_offsets.iter().any(|o| *o < 0) {
            return Err("remindOffsets 不能为负".to_string());
        }
        if self.catch_up_min < 0 {
            return Err("catchUpMin 不能为负".to_string());
        }
        if self.digest_hour > 23 {
            return Err("digestHour 须在 0–23 之间".to_string());
        }
        Ok(())
    }

    pub fn smtp_configured(&self) -> bool {
        !self.smtp_host.trim().is_empty() && !self.mail_to.is_empty()
    }

    pub fn webhook_configured(&self) -> bool {
        !self.webhook_url.trim().is_empty()
    }
}

pub fn load_notify_config(state: &AppState) -> NotifyConfig {
    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => return NotifyConfig::default(),
    };
    match repo::get_config(&db, "notify") {
        Ok(Some(raw)) => serde_json::from_str(&raw).unwrap_or_default(),
        _ => NotifyConfig::default(),
    }
}

pub fn load_smtp_password(state: &AppState) -> Option<String> {
    let db = state.db.lock().ok()?;
    let blob = repo::get_secret(&db, "smtp_password").ok()??;
    let plain = decrypt(&state.master_key, &blob).ok()?;
    String::from_utf8(plain).ok()
}

pub fn load_webhook_secret(state: &AppState) -> Option<String> {
    let db = state.db.lock().ok()?;
    let blob = repo::get_secret(&db, "webhook_secret").ok()??;
    let plain = decrypt(&state.master_key, &blob).ok()?;
    String::from_utf8(plain).ok()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyConfigOut {
    #[serde(flatten)]
    pub config: NotifyConfig,
    pub has_smtp_password: bool,
    pub has_webhook_secret: bool,
}

pub async fn get_notify(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<NotifyConfigOut>> {
    require_admin(&state, &headers)?;
    let config = load_notify_config(&state);
    let has_smtp_password = load_smtp_password(&state).is_some();
    let has_webhook_secret = load_webhook_secret(&state).is_some();
    Ok(Json(NotifyConfigOut {
        config,
        has_smtp_password,
        has_webhook_secret,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutNotifyInput {
    #[serde(flatten)]
    pub config: NotifyConfig,
    /// Some(非空) → 加密保存；Some("") → 删除；None → 不动。
    pub smtp_password: Option<String>,
    pub webhook_secret: Option<String>,
}

pub async fn put_notify(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<PutNotifyInput>,
) -> ApiResult<Json<NotifyConfigOut>> {
    require_admin(&state, &headers)?;
    input
        .config
        .validate()
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e))?;
    {
        let db = state
            .db
            .lock()
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "db locked"))?;
        let raw = serde_json::to_string(&input.config)
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "config serialize failed"))?;
        repo::set_config(&db, "notify", &raw)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
        match input.smtp_password.as_deref() {
            Some("") => {
                repo::delete_secret(&db, "smtp_password")
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            }
            Some(pw) => {
                let enc = encrypt(&state.master_key, pw.as_bytes())
                    .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "encrypt failed"))?;
                repo::set_secret(&db, "smtp_password", &enc)
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            }
            None => {}
        }
        match input.webhook_secret.as_deref() {
            Some("") => {
                repo::delete_secret(&db, "webhook_secret")
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            }
            Some(secret) => {
                let enc = encrypt(&state.master_key, secret.as_bytes())
                    .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "encrypt failed"))?;
                repo::set_secret(&db, "webhook_secret", &enc)
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            }
            None => {}
        }
    }
    let config = load_notify_config(&state);
    Ok(Json(NotifyConfigOut {
        config,
        has_smtp_password: load_smtp_password(&state).is_some(),
        has_webhook_secret: load_webhook_secret(&state).is_some(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestMailInput {
    pub to: Option<String>,
}

/// POST /api/test/mail：无 SMTP 配置时返回友好 400，不编造"已发送"。
pub async fn test_mail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<TestMailInput>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&state, &headers)?;
    let config = load_notify_config(&state);
    if config.smtp_host.trim().is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "未配置 SMTP 服务器，请先在通知配置里填写 smtpHost",
        ));
    }
    let to = input
        .to
        .or_else(|| config.mail_to.first().cloned())
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "未配置收件人（mailTo）"))?;
    let password = load_smtp_password(&state);
    let from = if config.mail_from.trim().is_empty() {
        config.smtp_user.clone()
    } else {
        config.mail_from.clone()
    };
    if from.trim().is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "未配置发件人（mailFrom 或 smtpUser）",
        ));
    }
    dispatcher::send_mail(
        &config,
        password.as_deref(),
        &from,
        &[to],
        "[待办] 测试邮件",
        "这是一封来自 Todo 服务端的测试邮件。",
        "<p>这是一封来自 Todo 服务端的测试邮件。</p>",
    )
    .await
    .map_err(|e| {
        let msg = friendly_mail_error(&e);
        err(StatusCode::BAD_GATEWAY, &msg)
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn friendly_mail_error(e: &str) -> String {
    if e.contains("535") || e.contains("authentication") || e.contains("Authentication") {
        "邮件认证失败：请用授权码而非登录密码".to_string()
    } else if e.contains("timed out") || e.contains("timeout") || e.contains("Connection refused") {
        "SMTP 连接超时/被拒：465/25 可能被封，建议 587 + STARTTLS".to_string()
    } else {
        format!("发送邮件失败：{e}")
    }
}

/// POST /api/test/webhook：发一次 ping 事件，不重试。
pub async fn test_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&state, &headers)?;
    let config = load_notify_config(&state);
    if config.webhook_url.trim().is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "未配置 Webhook 地址，请先在通知配置里填写 webhookUrl",
        ));
    }
    let secret = load_webhook_secret(&state);
    let status = dispatcher::send_webhook_once(
        &state.http,
        &config.webhook_url,
        secret.as_deref(),
        "ping",
        &serde_json::json!({ "event": "ping" }),
    )
    .await
    .map_err(|e| err(StatusCode::BAD_GATEWAY, &format!("Webhook 发送失败：{e}")))?;
    Ok(Json(
        serde_json::json!({ "ok": status.is_success(), "status": status.as_u16() }),
    ))
}
