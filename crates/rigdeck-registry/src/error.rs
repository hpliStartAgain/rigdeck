//! Registry 稳定错误。

/// Registry 结果。
pub type RegistryResult<T> = Result<T, RegistryError>;

/// 可映射到 CLI/UI 状态的 Registry 错误。
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// 来源定位符或响应无效。
    #[error("来源无效：{0}")]
    InvalidSource(String),
    /// 本地或远端资产不存在。
    #[error("资产不存在：{0}")]
    NotFound(String),
    /// 来源需要凭据。
    #[error("来源需要认证：{0}")]
    AuthenticationRequired(String),
    /// 远端限流。
    #[error("来源已限流，建议 {retry_after_seconds} 秒后重试：{message}")]
    RateLimited {
        /// `Retry-After` 或保守默认秒数。
        retry_after_seconds: u64,
        /// 服务端说明。
        message: String,
    },
    /// 网络不可用且没有可用缓存。
    #[error("远端暂不可用：{0}")]
    Offline(String),
    /// HTTP 状态或响应结构不符合 provider 合同。
    #[error("远端响应无效：{0}")]
    BadResponse(String),
    /// 恶意或超限归档/目录。
    #[error("安全策略拒绝：{0}")]
    Security(String),
    /// 文件系统错误。
    #[error("文件错误（{path}）：{source}")]
    Io {
        /// 相关路径。
        path: std::path::PathBuf,
        /// 原始错误。
        #[source]
        source: std::io::Error,
    },
    /// JSON 错误。
    #[error("JSON 编解码失败：{0}")]
    Json(#[from] serde_json::Error),
}

impl RegistryError {
    /// 构造带路径 I/O 错误。
    pub fn io(path: impl Into<std::path::PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
