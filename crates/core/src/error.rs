use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("记录不存在: {0}")]
    NotFound(String),
    #[error("参数无效: {0}")]
    Invalid(String),
    #[error("不允许的操作: {0}")]
    Forbidden(String),
    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
