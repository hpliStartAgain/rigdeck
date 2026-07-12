//! `adapter.json` 领域结构与语义验证。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{AdapterError, AdapterErrorCode, AdapterResult, ADAPTER_PROTOCOL_VERSION};
use rigdeck_core::{AssetKind, SurfaceMode};

/// Adapter 支持的平台。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// Windows。
    Windows,
    /// macOS。
    Macos,
    /// Linux（v1 非 GA blocker）。
    Linux,
}

impl Platform {
    /// 返回当前编译平台。
    pub const fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(target_os = "macos")]
        {
            Self::Macos
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Self::Linux
        }
    }
}

/// Adapter 协议兼容范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRange {
    /// 最低支持版本。
    pub min: u32,
    /// 最高支持版本。
    pub max: u32,
}

/// 原生能力操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityOperation {
    /// 导入现状。
    Import,
    /// 安装。
    Install,
    /// 更新。
    Update,
    /// 启用/禁用。
    Toggle,
    /// 删除。
    Remove,
    /// 检测外部漂移。
    Drift,
}

/// 某类资产在某作用域的能力声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetCapability {
    /// 资产种类。
    pub asset_kind: AssetKind,
    /// 支持的 scope ID。
    pub scopes: Vec<String>,
    /// 支持的操作。
    pub operations: BTreeSet<CapabilityOperation>,
}

/// 适配器作用域描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeDescriptor {
    /// 稳定 scope ID，例如 `global`、`project`。
    pub id: String,
    /// 用户可读名称。
    pub display_name: String,
    /// 是否需要 project root。
    pub project_required: bool,
}

/// 声明式检测规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionRule {
    /// 路径模板，允许 `{home}`、`{project}`。
    pub path: String,
    /// 该目录/文件存在后还必须存在的相对 marker。
    #[serde(default)]
    pub markers: Vec<String>,
    /// 多实例 profile 名；空表示默认实例。
    pub profile: Option<String>,
    /// 可选版本提取提示。声明式检测器不会执行命令。
    pub version_hint: Option<String>,
}

/// 原生配置格式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeFormat {
    /// 格式 ID，例如 `jsonc`、`toml`。
    pub id: String,
    /// 是否能保留注释。
    pub preserves_comments: bool,
    /// 是否能保留未知字段。
    pub preserves_unknown_fields: bool,
}

/// Codec 声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodecDescriptor {
    /// Codec ID。
    pub id: String,
    /// 输入的统一资产种类。
    pub asset_kind: AssetKind,
    /// 输出 native format ID。
    pub native_format: String,
}

/// 声明式原生管理表面。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceDescriptor {
    /// Adapter 内稳定 surface ID。
    pub id: String,
    /// 对应已声明 scope。
    pub scope: String,
    /// 接受的资产种类。
    pub asset_kind: AssetKind,
    /// 以 `{home}` 或 `{project}` 开头的允许根路径。
    pub root: String,
    /// 相对 root 的目标模板；只允许可选的 `{name}`。
    pub target: String,
    /// 原生格式 ID。
    pub native_format: String,
    /// 结构化配置的顶层 section；其他模式必须省略。
    pub section: Option<String>,
    /// 通用投影方式。
    pub mode: SurfaceMode,
    /// 是否允许创建新投影；旧兼容路径应为 `false`。
    pub writable: bool,
    /// 数字越小，发现冲突时越优先。
    pub precedence: u16,
}

/// 可见限制。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limitation {
    /// 稳定限制代码。
    pub code: String,
    /// 说明。
    pub message: String,
    /// 恢复或人工替代路径。
    pub recovery: String,
}

/// 可选 helper 描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperDescriptor {
    /// Adapter 包内相对可执行路径。
    pub executable: String,
    /// helper 文件 Blake3 hash。
    pub hash: String,
    /// helper 协议必须是 JSON-RPC 2.0。
    pub protocol: String,
    /// 向用户披露的请求权限。
    pub requested_access: Vec<String>,
}

/// Adapter 弃用声明；存在时 Core 向用户提示并引导迁移。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deprecation {
    /// 计划移除的 adapter 包版本。
    pub sunset_version: String,
    /// 替代 adapter ID；无替代时为 `None`。
    pub replacement: Option<String>,
    /// 弃用说明或迁移指引 URL。
    pub notice: String,
}

/// 完整 Adapter manifest。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterManifest {
    /// Manifest schema 版本。
    pub schema_version: u32,
    /// 小写稳定 adapter ID。
    pub adapter_id: String,
    /// Adapter 包版本。
    pub version: String,
    /// 用户可读名称。
    pub display_name: String,
    /// 协议兼容范围。
    pub protocol: ProtocolRange,
    /// 支持平台。
    pub platforms: BTreeSet<Platform>,
    /// 检测规则。
    pub detection: Vec<DetectionRule>,
    /// 资产能力矩阵。
    pub capabilities: Vec<AssetCapability>,
    /// 作用域。
    pub scopes: Vec<ScopeDescriptor>,
    /// 原生格式。
    pub native_formats: Vec<NativeFormat>,
    /// Codec。
    pub codecs: Vec<CodecDescriptor>,
    /// 资产作用域到本地路径的声明式映射。
    #[serde(default)]
    pub surfaces: Vec<SurfaceDescriptor>,
    /// 已知限制。
    #[serde(default)]
    pub limitations: Vec<Limitation>,
    /// 官方文档 URL。
    pub official_docs: Vec<String>,
    /// 可选 helper；加载 manifest 不会执行它。
    pub helper: Option<HelperDescriptor>,
    /// 可选弃用声明；存在时 Core 在加载和计划阶段向用户发出可见警告。
    #[serde(default)]
    pub deprecation: Option<Deprecation>,
}

impl AdapterManifest {
    /// 执行 JSON Schema 之外的跨字段语义验证。
    pub fn validate(&self) -> AdapterResult<()> {
        let id_valid = !self.adapter_id.is_empty()
            && self.adapter_id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte)
            });
        if !id_valid {
            return Err(AdapterError::new(
                AdapterErrorCode::InvalidManifest,
                "adapter_id 只能使用小写 ASCII、数字、点、横线和下划线",
            ));
        }
        if self.schema_version != 1 {
            return Err(AdapterError::new(
                AdapterErrorCode::InvalidManifest,
                format!("不支持 manifest schema {}", self.schema_version),
            ));
        }
        if self.protocol.min > ADAPTER_PROTOCOL_VERSION
            || self.protocol.max < ADAPTER_PROTOCOL_VERSION
            || self.protocol.min > self.protocol.max
        {
            return Err(AdapterError::new(
                AdapterErrorCode::IncompatibleProtocol,
                format!(
                    "Adapter 协议范围 {}..={} 不包含 Core 版本 {}",
                    self.protocol.min, self.protocol.max, ADAPTER_PROTOCOL_VERSION
                ),
            ));
        }
        if self.platforms.is_empty() || self.detection.is_empty() {
            return Err(AdapterError::new(
                AdapterErrorCode::InvalidManifest,
                "platforms 和 detection 不能为空",
            ));
        }
        for rule in &self.detection {
            validate_template(&rule.path)?;
            for marker in &rule.markers {
                validate_relative(marker, "marker")?;
            }
        }
        let scopes: BTreeSet<_> = self.scopes.iter().map(|scope| scope.id.as_str()).collect();
        let formats: BTreeSet<_> = self
            .native_formats
            .iter()
            .map(|format| format.id.as_str())
            .collect();
        for capability in &self.capabilities {
            for scope in &capability.scopes {
                if !scopes.contains(scope.as_str()) {
                    return Err(AdapterError::new(
                        AdapterErrorCode::InvalidManifest,
                        format!("能力引用了未声明 scope：{scope}"),
                    ));
                }
            }
        }
        let mut surface_ids = BTreeSet::new();
        for surface in &self.surfaces {
            if !surface_ids.insert(surface.id.as_str()) {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    format!("surface ID 重复：{}", surface.id),
                ));
            }
            if !scopes.contains(surface.scope.as_str()) {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    format!("surface 引用了未声明 scope：{}", surface.scope),
                ));
            }
            if !formats.contains(surface.native_format.as_str()) {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    format!(
                        "surface 引用了未声明 native format：{}",
                        surface.native_format
                    ),
                ));
            }
            match surface.mode {
                SurfaceMode::StructuredEntry
                    if surface.section.as_deref().unwrap_or("").is_empty() =>
                {
                    return Err(AdapterError::new(
                        AdapterErrorCode::InvalidManifest,
                        format!("结构化 surface {} 必须声明 section", surface.id),
                    ));
                }
                SurfaceMode::StructuredEntry => {}
                _ if surface.section.is_some() => {
                    return Err(AdapterError::new(
                        AdapterErrorCode::InvalidManifest,
                        format!("非结构化 surface {} 不能声明 section", surface.id),
                    ));
                }
                _ => {}
            }
            validate_template(&surface.root)?;
            validate_target_template(&surface.target)?;
            let supported = self.capabilities.iter().any(|capability| {
                capability.asset_kind == surface.asset_kind
                    && capability.scopes.contains(&surface.scope)
            });
            if !supported {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    format!("surface {} 没有对应的 capability", surface.id),
                ));
            }
        }
        if let Some(helper) = &self.helper {
            validate_relative(&helper.executable, "helper executable")?;
            if helper.protocol != "jsonrpc-2.0" {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    "helper protocol 必须是 jsonrpc-2.0",
                ));
            }
            if helper.hash.len() != 64 || !helper.hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    "helper hash 必须是 64 位 Blake3 十六进制字符串",
                ));
            }
        }
        if let Some(deprecation) = &self.deprecation {
            if deprecation.sunset_version.is_empty() {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    "deprecation.sunset_version 不能为空",
                ));
            }
            if deprecation.notice.is_empty() {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    "deprecation.notice 不能为空",
                ));
            }
            if let Some(replacement) = &deprecation.replacement {
                if replacement == &self.adapter_id {
                    return Err(AdapterError::new(
                        AdapterErrorCode::InvalidManifest,
                        "deprecation.replacement 不能指向自身",
                    ));
                }
                if !replacement
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte))
                {
                    return Err(AdapterError::new(
                        AdapterErrorCode::InvalidManifest,
                        "deprecation.replacement 只能使用小写 ASCII、数字、点、横线和下划线",
                    ));
                }
            }
        }
        Ok(())
    }

    /// 返回弃用状态；`Some` 表示已弃用，调用方应向用户发出可见警告。
    pub fn deprecation(&self) -> Option<&Deprecation> {
        self.deprecation.as_ref()
    }

    /// 当前平台是否可用。
    pub fn supports_current_platform(&self) -> bool {
        self.platforms.contains(&Platform::current())
    }
}

fn validate_target_template(value: &str) -> AdapterResult<()> {
    let probe = value.replace("{name}", "safe-name");
    if probe.contains('{') || probe.contains('}') {
        return Err(AdapterError::new(
            AdapterErrorCode::InvalidManifest,
            format!("target 只能使用 `{{name}}` 变量：{value}"),
        ));
    }
    validate_relative(&probe, "surface target")
}

fn validate_template(value: &str) -> AdapterResult<()> {
    if !(value.starts_with("{home}") || value.starts_with("{project}")) {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "检测 path 必须从 {home} 或 {project} 开始",
        ));
    }
    let suffix = value
        .trim_start_matches("{home}")
        .trim_start_matches("{project}")
        .trim_start_matches(['/', '\\']);
    validate_relative(suffix, "检测 path")
}

fn validate_relative(value: &str, label: &str) -> AdapterResult<()> {
    let path = camino::Utf8Path::new(value);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                camino::Utf8Component::ParentDir | camino::Utf8Component::Prefix(_)
            )
        })
    {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("{label} 不能是绝对路径或包含 `..`：{value}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_manifest() -> AdapterManifest {
        AdapterManifest {
            schema_version: 1,
            adapter_id: "mock-agent".to_owned(),
            version: "1.0.0".to_owned(),
            display_name: "Mock Agent".to_owned(),
            protocol: ProtocolRange { min: 1, max: 1 },
            platforms: BTreeSet::from([Platform::current()]),
            detection: vec![DetectionRule {
                path: "{home}/.mock-agent".to_owned(),
                markers: vec!["installed.marker".to_owned()],
                profile: None,
                version_hint: None,
            }],
            capabilities: vec![AssetCapability {
                asset_kind: AssetKind::Skill,
                scopes: vec!["global".to_owned()],
                operations: BTreeSet::from([CapabilityOperation::Install]),
            }],
            scopes: vec![ScopeDescriptor {
                id: "global".to_owned(),
                display_name: "全局".to_owned(),
                project_required: false,
            }],
            native_formats: vec![NativeFormat {
                id: "directory".to_owned(),
                preserves_comments: true,
                preserves_unknown_fields: true,
            }],
            codecs: Vec::new(),
            surfaces: Vec::new(),
            limitations: Vec::new(),
            official_docs: vec!["https://example.invalid/mock".to_owned()],
            helper: None,
            deprecation: None,
        }
    }

    #[test]
    fn deprecation_with_replacement_validates() {
        let mut manifest = base_manifest();
        manifest.deprecation = Some(Deprecation {
            sunset_version: "2.0.0".to_owned(),
            replacement: Some("mock-agent-v2".to_owned()),
            notice: "请迁移到 mock-agent-v2".to_owned(),
        });
        assert!(manifest.validate().is_ok());
        assert!(manifest.deprecation().is_some());
    }

    #[test]
    fn deprecation_without_replacement_validates() {
        let mut manifest = base_manifest();
        manifest.deprecation = Some(Deprecation {
            sunset_version: "2.0.0".to_owned(),
            replacement: None,
            notice: "不再维护，请手动迁移".to_owned(),
        });
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn deprecation_empty_sunset_fails() {
        let mut manifest = base_manifest();
        manifest.deprecation = Some(Deprecation {
            sunset_version: String::new(),
            replacement: None,
            notice: "notice".to_owned(),
        });
        assert_eq!(
            manifest.validate().unwrap_err().code,
            AdapterErrorCode::InvalidManifest
        );
    }

    #[test]
    fn deprecation_empty_notice_fails() {
        let mut manifest = base_manifest();
        manifest.deprecation = Some(Deprecation {
            sunset_version: "2.0.0".to_owned(),
            replacement: None,
            notice: String::new(),
        });
        assert_eq!(
            manifest.validate().unwrap_err().code,
            AdapterErrorCode::InvalidManifest
        );
    }

    #[test]
    fn deprecation_self_replacement_fails() {
        let mut manifest = base_manifest();
        manifest.deprecation = Some(Deprecation {
            sunset_version: "2.0.0".to_owned(),
            replacement: Some("mock-agent".to_owned()),
            notice: "notice".to_owned(),
        });
        assert_eq!(
            manifest.validate().unwrap_err().code,
            AdapterErrorCode::InvalidManifest
        );
    }

    #[test]
    fn deprecation_invalid_replacement_id_fails() {
        let mut manifest = base_manifest();
        manifest.deprecation = Some(Deprecation {
            sunset_version: "2.0.0".to_owned(),
            replacement: Some("Mock Agent".to_owned()),
            notice: "notice".to_owned(),
        });
        assert_eq!(
            manifest.validate().unwrap_err().code,
            AdapterErrorCode::InvalidManifest
        );
    }

    #[test]
    fn deprecation_round_trips_through_serde() {
        let mut manifest = base_manifest();
        manifest.deprecation = Some(Deprecation {
            sunset_version: "3.0.0".to_owned(),
            replacement: Some("new-agent".to_owned()),
            notice: "https://example.invalid/migrate".to_owned(),
        });
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: AdapterManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, parsed);
        assert!(parsed.deprecation().is_some());
    }

    #[test]
    fn manifest_without_deprecation_field_parses() {
        let mut manifest = base_manifest();
        let mut json: serde_json::Value =
            serde_json::to_value(&manifest).unwrap();
        // 模拟旧 manifest 不含 deprecation 字段
        json.as_object_mut().unwrap().remove("deprecation");
        let parsed: AdapterManifest = serde_json::from_value(json).unwrap();
        assert!(parsed.deprecation.is_none());
    }
}
