pub mod repo;

use rusqlite::Connection;

/// 打开（或创建）服务端数据库并建表。
///
/// 服务端不启用外键约束：同步时任务可能先于其列表到达（客户端按包内顺序推，
/// 跨设备也可能先收到任务），外键会让这条合法记录入库失败；孤儿任务由客户端
/// 拉取时按 §12 跳过、服务端不拦。`integrity_check` 失败时按 §12 改名备份后重建。
pub fn open(path: &std::path::Path) -> Result<Connection, rusqlite::Error> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                    Some(e.to_string()),
                )
            })?;
        }
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(include_str!("schema.sql"))?;
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        drop(conn);
        let backup = path.with_extension(format!(
            "corrupt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        ));
        let _ = std::fs::rename(path, &backup);
        let conn = Connection::open(path)?;
        conn.execute_batch(include_str!("schema.sql"))?;
        init_meta(&conn)?;
        return Ok(conn);
    }
    init_meta(&conn)?;
    Ok(conn)
}

fn init_meta(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('rev_counter', '0')",
        [],
    )?;
    // v4.4：库代际标识。缺失时生成一个 uuid v4；已存在时原样保留——
    // 只有删库文件重建才会生成新的，客户端凭它发现服务端换库（§9.5）。
    let generation = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('generation', ?1)",
        rusqlite::params![generation],
    )?;
    Ok(())
}
