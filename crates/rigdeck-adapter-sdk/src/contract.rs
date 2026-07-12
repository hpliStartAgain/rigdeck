//! Adapter 行为契约。

use camino::Utf8PathBuf;
use rigdeck_core::{
    AgentInstance, Asset, AssetRevision, ContentHash, ObservationIssue, Projection, SecretRef,
};
use serde::{Deserialize, Serialize};

use crate::{AdapterError, AdapterErrorCode, AdapterManifest, AdapterResult};

/// 检测上下文。
#[derive(Debug, Clone)]
pub struct DetectionContext {
    /// 用户 home。
    pub home: Utf8PathBuf,
    /// 可选当前 project root。
    pub project_root: Option<Utf8PathBuf>,
}

/// 扫描到的 Agent 原生条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannedEntry {
    /// 原生路径。
    pub path: Utf8PathBuf,
    /// 推断资产种类。
    pub kind: rigdeck_core::AssetKind,
    /// raw hash。
    pub raw_hash: ContentHash,
    /// normalized hash。
    pub normalized_hash: ContentHash,
    /// 是否包含 RigDeck managed marker。
    pub managed: bool,
    /// 可选稳定逻辑资产 ID。
    #[serde(default)]
    pub logical_id: Option<String>,
    /// 扫描到的结构或能力问题。
    #[serde(default)]
    pub issue: Option<ObservationIssue>,
}

/// 资产适配验证结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetValidation {
    /// 是否可投影。
    pub valid: bool,
    /// 警告代码/说明。
    pub warnings: Vec<String>,
    /// 阻断原因。
    pub errors: Vec<String>,
}

/// Adapter 返回给 Core Planner 的操作意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterPlan {
    /// 要写入的投影。
    pub projection: Option<Projection>,
    /// 要删除的显式绝对目标。
    pub remove_targets: Vec<Utf8PathBuf>,
    /// 从共享文件中精确移除托管内容的意图。
    #[serde(default)]
    pub removals: Vec<RemovalIntent>,
    /// 需要在外部 UI 完成的精确步骤。
    pub manual_required: Vec<String>,
}

/// 精确卸载一个投影，而不是默认删除整个共享文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemovalIntent {
    /// 目标绝对路径。
    pub target_path: Utf8PathBuf,
    /// 与安装投影对称的移除策略。
    pub strategy: RemovalStrategy,
}

/// 共享目标的精确移除策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemovalStrategy {
    /// 目标完全由资产拥有，可以删除文件或 Skill 目录。
    RemoveOwnedPath,
    /// 只删除指定托管块。
    ManagedBlock {
        /// 稳定资产块 ID。
        block_id: String,
    },
    /// 只删除指定结构化配置条目。
    StructuredEntry {
        /// 顶层 section。
        section: String,
        /// section 内 key。
        entry_key: String,
    },
}

/// Adapter 新渲染出的内存对象。
///
/// 这些字节只包含资产内容或 `SecretRef`，不会包含从钥匙串物化的 secret。调用方必须
/// 校验 hash 后写入加密对象库；自定义 `Debug` 故意不输出内容。
#[derive(Clone, PartialEq, Eq)]
pub struct RenderedObject {
    /// 内容 hash。
    pub hash: ContentHash,
    /// 待写入对象库的字节。
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for RenderedObject {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RenderedObject")
            .field("hash", &self.hash)
            .field("byte_len", &self.bytes.len())
            .finish()
    }
}

/// 一次纯渲染的投影和待持久化对象。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutput {
    /// 可安全持久化的投影元数据。
    pub projection: Projection,
    /// 由本次渲染产生、尚未写入对象库的内容。
    pub objects: Vec<RenderedObject>,
}

/// 传给 Adapter 的一个资产文件。
///
/// 字节内容不实现 `Serialize`，也不会在 `Debug` 中展开，避免把未信任内容或未来的
/// 临时 secret 错误送入日志。路径必须由 bundle 导入器先完成安全校验。
#[derive(Clone, PartialEq, Eq)]
pub struct AssetFileContent {
    /// 资产根目录内的相对路径。
    pub relative_path: Utf8PathBuf,
    /// 原始文件字节。
    pub bytes: Vec<u8>,
    /// 来源是否声明可执行位；投影默认不会自动执行。
    pub executable: bool,
}

impl std::fmt::Debug for AssetFileContent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AssetFileContent")
            .field("relative_path", &self.relative_path)
            .field("byte_len", &self.bytes.len())
            .field("executable", &self.executable)
            .finish()
    }
}

/// 完整资产文件 bundle。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetContent {
    /// 按相对路径唯一标识的文件。
    pub files: Vec<AssetFileContent>,
}

impl AssetContent {
    /// 创建单文件内容，供 Prompt/MCP 和旧调用方使用。
    pub fn single(relative_path: impl Into<Utf8PathBuf>, bytes: Vec<u8>) -> Self {
        Self {
            files: vec![AssetFileContent {
                relative_path: relative_path.into(),
                bytes,
                executable: false,
            }],
        }
    }

    /// 计算路径、原始内容 hash 和 executable 位共同决定的 bundle hash。
    pub fn raw_hash(&self) -> AdapterResult<ContentHash> {
        bundle_hash(self, false)
    }

    /// 计算文本换行规范化后的 bundle hash。
    pub fn normalized_hash(&self) -> AdapterResult<ContentHash> {
        bundle_hash(self, true)
    }

    /// 生成可存入对象库的确定性文件清单 JSON。
    pub fn inventory_bytes(&self) -> AdapterResult<Vec<u8>> {
        let mut files: Vec<_> = self.files.iter().collect();
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let inventory: Vec<_> = files
            .into_iter()
            .map(|file| {
                serde_json::json!({
                    "path": file.relative_path,
                    "raw_hash": ContentHash::from_bytes(&file.bytes),
                    "normalized_hash": rigdeck_core::normalized_hash(&file.bytes),
                    "size": file.bytes.len(),
                    "executable": file.executable,
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "files": inventory,
        }))
        .map_err(|error| {
            AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                format!("Asset inventory 无法序列化：{error}"),
            )
        })
    }
}

/// 投影验证结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    /// 是否与预期一致。
    pub valid: bool,
    /// 不一致说明。
    pub differences: Vec<String>,
}

/// Adapter 健康检查结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterHealthReport {
    /// 是否可正常使用。
    pub healthy: bool,
    /// 健康检查项。
    pub checks: Vec<String>,
}

/// Adapter helper 信任决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperTrust {
    /// Adapter ID。
    pub adapter_id: String,
    /// 被信任 helper 的 hash。
    pub helper_hash: ContentHash,
    /// 用户批准的访问说明。
    pub approved_access: Vec<String>,
}

/// 所有运行时 Adapter 必须实现的方法。
///
/// `Send + Sync` 表示实现可安全在线程间移动和共享，便于桌面后台扫描。方法均接收
/// `&self` 借用，Adapter 不会因为一次调用而被消费。
pub trait AgentAdapter: Send + Sync {
    /// 描述 manifest。
    fn describe(&self) -> &AdapterManifest;
    /// 检测实例。
    fn detect(&self, context: &DetectionContext) -> AdapterResult<Vec<AgentInstance>>;
    /// 扫描原生状态。
    fn scan(&self, instance: &AgentInstance) -> AdapterResult<Vec<ScannedEntry>>;
    /// 验证资产兼容性。
    fn validate_asset(
        &self,
        asset: &Asset,
        revision: &AssetRevision,
        instance: &AgentInstance,
        scope: &str,
    ) -> AdapterResult<AssetValidation>;
    /// 渲染为目标原生格式，内容写入对象库而不是目标文件。
    fn render(
        &self,
        asset: &Asset,
        revision: &AssetRevision,
        content: &[u8],
        instance: &AgentInstance,
        scope: &str,
        secret_bindings: &[SecretRef],
    ) -> AdapterResult<RenderOutput>;
    /// 渲染完整多文件资产；默认实现只接受单文件并委托给 [`Self::render`]。
    fn render_bundle(
        &self,
        asset: &Asset,
        revision: &AssetRevision,
        content: &AssetContent,
        instance: &AgentInstance,
        scope: &str,
        secret_bindings: &[SecretRef],
    ) -> AdapterResult<RenderOutput> {
        let [file] = content.files.as_slice() else {
            return Err(AdapterError::new(
                AdapterErrorCode::UnsupportedCapability,
                "该 Adapter 尚未实现多文件 bundle 渲染",
            ));
        };
        self.render(
            asset,
            revision,
            &file.bytes,
            instance,
            scope,
            secret_bindings,
        )
    }
    /// 规划安装。
    fn plan_install(&self, projection: &Projection) -> AdapterResult<AdapterPlan>;
    /// 规划更新。
    fn plan_update(&self, projection: &Projection) -> AdapterResult<AdapterPlan>;
    /// 规划移除。
    fn plan_remove(
        &self,
        asset: &Asset,
        instance: &AgentInstance,
        scope: &str,
    ) -> AdapterResult<AdapterPlan>;
    /// 验证应用结果。
    fn verify(&self, projection: &Projection) -> AdapterResult<VerificationReport>;
    /// Adapter 自身健康检查。
    fn health(&self, context: &DetectionContext) -> AdapterResult<AdapterHealthReport>;
}

fn bundle_hash(content: &AssetContent, normalized: bool) -> AdapterResult<ContentHash> {
    let mut files: Vec<_> = content.files.iter().collect();
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let mut seen = std::collections::BTreeSet::new();
    let mut material = Vec::new();
    for file in files {
        let path = &file.relative_path;
        if path.as_str().is_empty()
            || path.is_absolute()
            || path.components().any(|part| {
                matches!(
                    part,
                    camino::Utf8Component::ParentDir
                        | camino::Utf8Component::RootDir
                        | camino::Utf8Component::Prefix(_)
                )
            })
            || !seen.insert(path.clone())
        {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("Asset bundle 路径无效或重复：{path}"),
            ));
        }
        let hash = if normalized {
            rigdeck_core::normalized_hash(&file.bytes)
        } else {
            ContentHash::from_bytes(&file.bytes)
        };
        material.extend_from_slice(path.as_str().as_bytes());
        material.push(0);
        material.extend_from_slice(hash.as_str().as_bytes());
        material.push(u8::from(file.executable));
        material.push(0);
    }
    Ok(ContentHash::from_bytes(&material))
}
