//! 应用服务稳定错误。

/// 服务结果。
pub type ServiceResult<T> = Result<T, ServiceError>;

/// CLI 与 Tauri IPC 共用的服务错误。
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// 输入或环境无效。
    #[error("输入无效：{0}")]
    InvalidInput(String),
    /// 领域对象不存在。
    #[error("对象不存在：{0}")]
    NotFound(String),
    /// 操作需要人工步骤或额外确认。
    #[error("需要人工处理：{0}")]
    ManualRequired(String),
    /// Core 错误。
    #[error(transparent)]
    Core(#[from] rigdeck_core::CoreError),
    /// Store 错误。
    #[error(transparent)]
    Store(#[from] rigdeck_store::StoreError),
    /// Adapter 错误。
    #[error(transparent)]
    Adapter(#[from] rigdeck_adapter_sdk::AdapterError),
    /// Registry 错误。
    #[error(transparent)]
    Registry(#[from] rigdeck_registry::RegistryError),
    /// Keychain 错误。
    #[error(transparent)]
    Secret(#[from] rigdeck_security::SecretError),
    /// JSON 错误。
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// 后台工作被用户取消。
    #[error("操作已取消")]
    Cancelled,
}
