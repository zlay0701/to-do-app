pub mod repo;

use crate::error::Result;
use rusqlite::Connection;

/// 打开（或创建）客户端数据库并建表。
///
/// 首次启动还会插入"收件箱"（dirty=1，等待首次同步上推，SPEC §3）。
pub fn open(path: &std::path::Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| crate::error::CoreError::Invalid(e.to_string()))?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(include_str!("schema.sql"))?;
    migrate(&conn)?;
    repo::ensure_inbox(&conn)?;
    Ok(conn)
}

/// 旧库迁移：schema.sql 只建缺失的表，不给已存在的表补列。
/// v4.4：sync_client_state 加 generation 列（代际标识，§9.5）；列已存在时忽略报错。
fn migrate(conn: &Connection) -> Result<()> {
    let _ = conn.execute(
        "ALTER TABLE sync_client_state ADD COLUMN generation TEXT",
        [],
    );
    Ok(())
}

/// 打开内存库（单测用）。
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(include_str!("schema.sql"))?;
    migrate(&conn)?;
    repo::ensure_inbox(&conn)?;
    Ok(conn)
}
