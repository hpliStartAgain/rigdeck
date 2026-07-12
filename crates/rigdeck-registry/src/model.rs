//! Provider、缓存与目录结果模型。

use rigdeck_adapter_sdk::AssetContent;
use rigdeck_core::{AssetKind, Source};
use serde::{Deserialize, Serialize};

/// 远端响应使用缓存的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheState {
    /// 直接来自网络。
    Network,
    /// 服务端返回 304，复用缓存。
    Revalidated,
    /// 网络失败后使用过期缓存。
    StaleOffline,
}

/// 远端限流元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitStatus {
    /// 总额度。
    pub limit: Option<u64>,
    /// 剩余额度。
    pub remaining: Option<u64>,
    /// Unix 秒 reset 时间或相对秒，取决于 provider 官方定义。
    pub reset: Option<u64>,
}

/// 一条来自 registry 或扫描器的漏洞信息；RigDeck 只展示，不等于已审计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VulnerabilityInfo {
    /// 稳定漏洞 ID，例如 CVE 或 GHSA 编号。
    pub id: String,
    /// 严重程度（来自来源的原文字符串）。
    pub severity: String,
    /// 可选受影响版本范围。
    pub affected_versions: Option<String>,
    /// 可选修复版本。
    pub fixed_in: Option<String>,
    /// 可选来源 URL。
    pub url: Option<String>,
}

/// 一个可展示的目录结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogItem {
    /// Provider 内稳定 ID。
    pub id: String,
    /// 来源 Provider ID。
    pub provider_id: String,
    /// 资产种类。
    pub kind: AssetKind,
    /// 名称。
    pub name: String,
    /// 可选说明。
    pub description: Option<String>,
    /// 可选版本。
    pub version: Option<String>,
    /// 可选许可证。
    pub license: Option<String>,
    /// 来源 URL 或公开定位符，不含凭据。
    pub locator: String,
    /// Namespace 是否由 registry 声明为已验证；不等于代码安全。
    pub namespace_verified: bool,
    /// Provider 原生 metadata，复制到 UI 前仍需转义。
    pub metadata: serde_json::Value,
    /// 已知漏洞信息；空表示 registry 未报告漏洞，不等于无漏洞。
    #[serde(default)]
    pub vulnerabilities: Vec<VulnerabilityInfo>,
}

/// 搜索结果和缓存/限流状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// 结果项。
    pub items: Vec<CatalogItem>,
    /// 缓存状态。
    pub cache_state: CacheState,
    /// 可选限流状态。
    pub rate_limit: Option<RateLimitStatus>,
}

/// 已获取、尚未持久化的完整资产内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedAsset {
    /// 资产种类。
    pub kind: AssetKind,
    /// 来源 provenance。
    pub source: Source,
    /// 来源 namespace。
    pub source_namespace: String,
    /// 包/仓库 ID。
    pub package: String,
    /// 包内相对资产根。
    pub relative_path: camino::Utf8PathBuf,
    /// 完整文件 bundle。
    pub content: AssetContent,
    /// 许可证提示。
    pub license: Option<String>,
    /// Provider metadata。
    pub metadata: serde_json::Value,
}

/// Provider 获取的完整目录对象。
#[derive(Debug, Clone, PartialEq)]
pub enum FetchedCatalogAsset {
    /// 可直接进入安全审计/导入的 Skill 文件树。
    Skill(FetchedAsset),
    /// MCP Registry 的标准 `server.json` metadata；不是已审计代码包。
    McpMetadata {
        /// 搜索/展示信息。
        item: CatalogItem,
        /// 原始标准 metadata。
        server_json: serde_json::Value,
    },
}
