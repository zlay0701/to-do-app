//! 提醒调度（SPEC §10）：每 60s tick 直接查权威库。
//!
//! 候选点 `due_at - offset`；命中窗口 `[now-tick, now+tick]`，错过的在
//! `catch_up_min` 内补发一次、超出丢弃；`reminder_ledger` 唯一键认领后才真正
//! 发送（INSERT OR IGNORE 影响行数=1），重启不重发；改期→新 anchor→可再提醒；
//! 完成后（status=1）不再提醒。摘要按 notify_config 的时区算"今天"。

use crate::api::config::{load_notify_config, load_smtp_password, load_webhook_secret};
use crate::db::repo;
use crate::jobs::dispatcher;
use crate::state::AppState;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use chrono_tz::Tz;
use std::sync::Arc;
use todo_core::now_ms;

/// 认领一条台账（锁失败或已认领过返回 None，不抛错）。
fn claim(
    state: &Arc<AppState>,
    task_id: &str,
    kind: &str,
    anchor: i64,
    offset_min: i64,
    channel: &str,
    now: i64,
) -> Option<i64> {
    let db = state.db.lock().ok()?;
    repo::claim_ledger(&db, task_id, kind, anchor, offset_min, channel, now)
        .ok()
        .flatten()
}

pub struct TickSummary {
    pub scanned: i64,
    pub enqueued: i64,
    pub sent: i64,
    pub failed: i64,
}

fn fmt_due(due_at: i64, tz: Tz) -> String {
    let dt = tz.timestamp_millis_opt(due_at).single();
    match dt {
        Some(d) => d.format("%Y-%m-%d %H:%M").to_string(),
        None => Utc
            .timestamp_millis_opt(due_at)
            .single()
            .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_default(),
    }
}

/// 跑一轮提醒 tick（可单独调用，便于测试与手动触发）。
pub async fn run_tick(state: &Arc<AppState>, now: i64) -> TickSummary {
    let config = load_notify_config(state);
    let tick_ms = state.poll_interval_secs as i64 * 1000;
    let catch_up_ms = config.catch_up_min * 60_000;
    let tz: Tz = config.timezone.parse().unwrap_or(chrono_tz::UTC);

    let run_id = {
        let Ok(db) = state.db.lock() else {
            return TickSummary {
                scanned: 0,
                enqueued: 0,
                sent: 0,
                failed: 0,
            };
        };
        repo::insert_run(&db, now).unwrap_or(0)
    };
    let mut summary = TickSummary {
        scanned: 0,
        enqueued: 0,
        sent: 0,
        failed: 0,
    };

    if !config.enabled {
        finish(state, run_id, now, &summary, Some("提醒已禁用"));
        return summary;
    }

    let tasks = {
        let Ok(db) = state.db.lock() else {
            finish(state, run_id, now, &summary, Some("db locked"));
            return summary;
        };
        repo::due_tasks(&db).unwrap_or_default()
    };
    summary.scanned = tasks.len() as i64;

    let smtp_password = load_smtp_password(state);
    let webhook_secret = load_webhook_secret(state);
    let mail_from = if config.mail_from.trim().is_empty() {
        config.smtp_user.clone()
    } else {
        config.mail_from.clone()
    };

    for task in &tasks {
        let Some(due_at) = task.due_at else { continue };
        for offset in &config.remind_offsets {
            let remind_at = due_at - offset * 60_000;
            if remind_at > now + tick_ms {
                continue; // 还没到提醒点
            }
            if remind_at < now - catch_up_ms {
                continue; // 错过太久，丢弃（§10 补偿窗口之外）
            }
            let event = if now > due_at {
                "task.overdue"
            } else {
                "task.due"
            };
            // 邮件通道
            if config.smtp_configured() {
                let claimed = claim(state, &task.id, "due", due_at, *offset, "mail", now);
                if let Some(ledger_id) = claimed {
                    summary.enqueued += 1;
                    let subject = format!("[待办] {}", task.title);
                    let due_str = fmt_due(due_at, tz);
                    let text = format!("任务「{}」将于 {} 到期。", task.title, due_str);
                    let html = format!(
                        "<p>任务<strong>{}</strong>将于 {} 到期。</p>",
                        html_escape(&task.title),
                        due_str
                    );
                    let result = dispatcher::send_mail(
                        &config,
                        smtp_password.as_deref(),
                        &mail_from,
                        &config.mail_to,
                        &subject,
                        &text,
                        &html,
                    )
                    .await;
                    record_send(state, ledger_id, result, now, &mut summary);
                }
            }
            // Webhook 通道
            if config.webhook_configured() {
                let claimed = claim(state, &task.id, "due", due_at, *offset, "webhook", now);
                if let Some(ledger_id) = claimed {
                    summary.enqueued += 1;
                    let payload = serde_json::json!({
                        "event": event,
                        "taskId": task.id,
                        "title": task.title,
                        "dueAt": due_at,
                        "sentAt": now,
                    });
                    let result = dispatcher::send_webhook_with_retry(
                        &state.http,
                        &config.webhook_url,
                        webhook_secret.as_deref(),
                        event,
                        &payload,
                    )
                    .await;
                    record_send(state, ledger_id, result, now, &mut summary);
                }
            }
        }
    }

    // 每日摘要：按配置时区的当天 digest_hour 时刻，窗口内发一次。
    if config.digest_enabled {
        run_digest(
            state,
            now,
            tick_ms,
            catch_up_ms,
            tz,
            &smtp_password,
            &webhook_secret,
            &mail_from,
            &mut summary,
        )
        .await;
    }

    finish(state, run_id, now, &summary, None);
    summary
}

fn record_send(
    state: &Arc<AppState>,
    ledger_id: i64,
    result: Result<(), String>,
    now: i64,
    summary: &mut TickSummary,
) {
    let Ok(db) = state.db.lock() else { return };
    match result {
        Ok(()) => {
            summary.sent += 1;
            let _ = repo::set_ledger_status(&db, ledger_id, 1, Some(now), None);
        }
        Err(e) => {
            summary.failed += 1;
            // 台账只记错误文本，不含凭据。
            let _ = repo::set_ledger_status(&db, ledger_id, 2, None, Some(&e));
        }
    }
}

fn finish(state: &Arc<AppState>, run_id: i64, now: i64, summary: &TickSummary, note: Option<&str>) {
    if run_id == 0 {
        return;
    }
    if let Ok(db) = state.db.lock() {
        let _ = repo::finish_run(
            &db,
            run_id,
            now,
            summary.scanned,
            summary.enqueued,
            summary.sent,
            summary.failed,
            note,
        );
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 摘要：今天（配置时区）到期的未完成任务，digest_hour 时刻发一次。
#[allow(clippy::too_many_arguments)]
async fn run_digest(
    state: &Arc<AppState>,
    now: i64,
    tick_ms: i64,
    catch_up_ms: i64,
    tz: Tz,
    smtp_password: &Option<String>,
    webhook_secret: &Option<String>,
    mail_from: &str,
    summary: &mut TickSummary,
) {
    let config = load_notify_config(state);
    let today: DateTime<Tz> = tz
        .timestamp_millis_opt(now)
        .single()
        .unwrap_or_else(|| Utc::now().with_timezone(&tz));
    let digest_at = tz
        .with_ymd_and_hms(
            today.date_naive().year(),
            today.date_naive().month(),
            today.date_naive().day(),
            config.digest_hour as u32,
            0,
            0,
        )
        .single()
        .map(|d| d.timestamp_millis());
    let Some(digest_at) = digest_at else { return };
    if digest_at > now + tick_ms || digest_at < now - catch_up_ms {
        return;
    }
    let tasks = {
        let Ok(db) = state.db.lock() else { return };
        repo::due_tasks(&db).unwrap_or_default()
    };
    let today_tasks: Vec<_> = tasks
        .into_iter()
        .filter(|t| {
            t.due_at
                .map(|d| {
                    tz.timestamp_millis_opt(d)
                        .single()
                        .map(|dt| dt.date_naive() == today.date_naive())
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        })
        .collect();
    if today_tasks.is_empty() {
        return;
    }
    let anchor = today
        .date_naive()
        .format("%Y%m%d")
        .to_string()
        .parse::<i64>()
        .unwrap_or(0);
    let count = today_tasks.len();
    if config.smtp_configured() {
        let claimed = claim(state, "__digest__", "digest", anchor, 0, "mail", now);
        if let Some(ledger_id) = claimed {
            summary.enqueued += 1;
            let subject = format!("[待办] 今日 {count} 项待办");
            let lines: Vec<String> = today_tasks
                .iter()
                .map(|t| {
                    format!(
                        "- {}（{}）",
                        t.title,
                        t.due_at.map(|d| fmt_due(d, tz)).unwrap_or_default()
                    )
                })
                .collect();
            let text = format!("今日待办（{count} 项）：\n{}", lines.join("\n"));
            let html = format!(
                "<p>今日待办（{} 项）：</p><ul>{}</ul>",
                count,
                today_tasks
                    .iter()
                    .map(|t| format!("<li>{}</li>", html_escape(&t.title)))
                    .collect::<Vec<_>>()
                    .join("")
            );
            let result = dispatcher::send_mail(
                &config,
                smtp_password.as_deref(),
                mail_from,
                &config.mail_to,
                &subject,
                &text,
                &html,
            )
            .await;
            record_send(state, ledger_id, result, now, summary);
        }
    }
    if config.webhook_configured() {
        let claimed = claim(state, "__digest__", "digest", anchor, 0, "webhook", now);
        if let Some(ledger_id) = claimed {
            summary.enqueued += 1;
            let payload = serde_json::json!({
                "event": "digest.daily",
                "count": count,
                "tasks": today_tasks.iter().map(|t| serde_json::json!({
                    "taskId": t.id, "title": t.title, "dueAt": t.due_at
                })).collect::<Vec<_>>(),
                "sentAt": now,
            });
            let result = dispatcher::send_webhook_with_retry(
                &state.http,
                &config.webhook_url,
                webhook_secret.as_deref(),
                "digest.daily",
                &payload,
            )
            .await;
            record_send(state, ledger_id, result, now, summary);
        }
    }
}

/// 后台循环：每 poll_interval 秒跑一轮 tick。
pub async fn scheduler_loop(state: Arc<AppState>) {
    let mut interval =
        tokio::time::interval(std::time::Duration::from_secs(state.poll_interval_secs));
    loop {
        interval.tick().await;
        let now = now_ms();
        run_tick(&state, now).await;
    }
}
