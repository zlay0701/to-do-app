//! 提醒发送：SMTP（lettre）与 Webhook（HMAC 签名 + 重试，SPEC §10）。
//!
//! Webhook：`X-Todo-Event` / `X-Todo-Signature: sha256=<hmac>`；2xx 成功；
//! 4xx（除 408/429）不重试；5xx/超时重试 3 次（30s / 2min / 10min）。

use crate::api::config::NotifyConfig;
use crate::crypto::hmac_sha256_hex;
use lettre::message::MultiPart;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use reqwest::StatusCode;

/// 发一封邮件（纯文本 + HTML，不含笔记全文）。
pub async fn send_mail(
    config: &NotifyConfig,
    smtp_password: Option<&str>,
    from: &str,
    to: &[String],
    subject: &str,
    text: &str,
    html: &str,
) -> Result<(), String> {
    let mut builder =
        Message::builder().from(from.parse().map_err(|e| format!("发件人地址无效：{e}"))?);
    for addr in to {
        builder = builder.to(addr.parse().map_err(|e| format!("收件人地址无效：{e}"))?);
    }
    let email = builder
        .subject(subject)
        .multipart(MultiPart::alternative_plain_html(
            text.to_string(),
            html.to_string(),
        ))
        .map_err(|e| format!("构造邮件失败：{e}"))?;

    // 465 用隐式 TLS，587 及其他端口用 STARTTLS。
    let builder = if config.smtp_port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
            .map_err(|e| format!("SMTP 配置失败：{e}"))?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)
            .map_err(|e| format!("SMTP 配置失败：{e}"))?
    };
    let builder = builder.port(config.smtp_port);
    let builder = if !config.smtp_user.is_empty() {
        builder.credentials(Credentials::new(
            config.smtp_user.clone(),
            smtp_password.unwrap_or_default().to_string(),
        ))
    } else {
        builder
    };
    let transport = builder.build();
    transport
        .send(email)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// 发一次 Webhook，返回对方 HTTP 状态（调用方决定是否重试）。
pub async fn send_webhook_once(
    http: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
    event: &str,
    payload: &serde_json::Value,
) -> Result<StatusCode, String> {
    let body = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let mut req = http
        .post(url)
        .header("X-Todo-Event", event)
        .header("Content-Type", "application/json")
        .body(body.clone());
    if let Some(secret) = secret {
        let signature = hmac_sha256_hex(secret.as_bytes(), &body);
        req = req.header("X-Todo-Signature", format!("sha256={signature}"));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    Ok(resp.status())
}

/// 带重试的 Webhook：5xx/超时重试 3 次（30s / 2min / 10min）；4xx（除 408/429）不重试。
pub async fn send_webhook_with_retry(
    http: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
    event: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    const DELAYS_SECS: [u64; 3] = [30, 120, 600];
    let mut last_err = String::new();
    for (attempt, delay) in DELAYS_SECS.iter().enumerate().chain([(3, &0)]) {
        match send_webhook_once(http, url, secret, event, payload).await {
            Ok(status) if status.is_success() => return Ok(()),
            Ok(status)
                if status.is_client_error()
                    && status != StatusCode::REQUEST_TIMEOUT
                    && status != StatusCode::TOO_MANY_REQUESTS =>
            {
                return Err(format!("Webhook 返回 HTTP {}", status.as_u16()));
            }
            Ok(status) => {
                last_err = format!("Webhook 返回 HTTP {}", status.as_u16());
            }
            Err(e) => {
                last_err = e;
            }
        }
        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
        }
    }
    Err(last_err)
}
