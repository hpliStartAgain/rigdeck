//! Core 统一错误类型。

use std::path::PathBuf;

/// Core 操作的统一结果别名。
pub type CoreResult<T> = Result<T, CoreError>;

/// 共享 Core 可以向 CLI、桌面端和测试稳定映射的错误。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// 输入不满足领域不变量。
    #[error("输入无效：{0}")]
    InvalidInput(String),
    /// 计划结构本身无效。
    #[error("部署计划无效：{0}")]
    InvalidPlan(String),
    /// 计划生成后目标文件已经变化。
    #[error("计划已失效，目标状态发生变化：{path}")]
    PlanInvalidated {
        /// 已变化的目标路径。
        path: PathBuf,
    },
    /// 内容寻址对象不存在或 hash 不匹配。
    #[error("内容对象不可用：{0}")]
    ObjectUnavailable(String),
    /// 写入后的验证结果不符合计划。
    #[error("应用后验证失败：{0}")]
    VerificationFailed(String),
    /// 文件回滚未能完整恢复。
    #[error("回滚失败：{0}")]
    RollbackFailed(String),
    /// 持久化提交失败。
    #[error("提交状态失败：{0}")]
    CommitFailed(String),
    /// 文件系统错误。
    #[error("文件系统错误（{path}）：{source}")]
    Io {
        /// 发生错误的路径。
        path: PathBuf,
        /// 原始系统错误。
        #[source]
        source: std::io::Error,
    },
    /// JSON 编解码错误。
    #[error("JSON 编解码失败：{0}")]
    Json(#[from] serde_json::Error),
}

impl CoreError {
    /// 为带路径的 I/O 错误补充上下文。
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
