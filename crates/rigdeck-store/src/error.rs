//! Store 错误。

use std::path::PathBuf;

/// Store 操作结果。
pub type StoreResult<T> = Result<T, StoreError>;

/// 持久化和对象库错误。
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite 错误。
    #[error("SQLite 错误：{0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Migration 错误。
    #[error("数据库 migration 失败：{0}")]
    Migration(#[from] refinery::Error),
    /// JSON 错误。
    #[error("JSON 编解码失败：{0}")]
    Json(#[from] serde_json::Error),
    /// I/O 错误。
    #[error("文件错误（{path}）：{source}")]
    Io {
        /// 路径。
        path: PathBuf,
        /// 原始错误。
        #[source]
        source: std::io::Error,
    },
    /// 对象不存在或损坏。
    #[error("内容对象错误：{0}")]
    Object(String),
    /// 尝试持久化明文 secret。
    #[error("secret 策略拒绝：{0}")]
    SecretPolicy(String),
    /// 数据库完整性检查失败。
    #[error("数据库完整性检查失败：{0}")]
    Integrity(String),
}

impl StoreError {
    /// 为 I/O 错误增加路径。
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
