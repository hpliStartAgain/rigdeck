//! Adapter 稳定错误。

use rigdeck_core::CoreError;

/// Adapter 操作结果。
pub type AdapterResult<T> = Result<T, AdapterError>;

/// 可映射到 CLI JSON/退出码的稳定错误代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterErrorCode {
    /// manifest 结构或语义无效。
    InvalidManifest,
    /// Adapter 协议与 Core 不兼容。
    IncompatibleProtocol,
    /// 路径越过允许根目录。
    PathViolation,
    /// 当前平台不受支持。
    UnsupportedPlatform,
    /// 目标能力不支持。
    UnsupportedCapability,
    /// 官方接口要求人工处理。
    ManualRequired,
    /// helper 未经信任。
    HelperNotTrusted,
    /// 扫描或验证失败。
    ValidationFailed,
    /// I/O 失败。
    Io,
    /// Core 错误。
    Core,
}

/// Adapter 错误，始终包含稳定代码和中文说明。
#[derive(Debug, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct AdapterError {
    /// 稳定机器代码。
    pub code: AdapterErrorCode,
    /// 用户可读说明。
    pub message: String,
    /// 可选恢复动作。
    pub recovery: Option<String>,
}

impl AdapterError {
    /// 创建错误。
    pub fn new(code: AdapterErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            recovery: None,
        }
    }

    /// 附加恢复说明。
    pub fn with_recovery(mut self, recovery: impl Into<String>) -> Self {
        self.recovery = Some(recovery.into());
        self
    }
}

impl From<CoreError> for AdapterError {
    fn from(error: CoreError) -> Self {
        Self::new(AdapterErrorCode::Core, error.to_string())
    }
}
