//! 统一领域模型。

use std::collections::BTreeMap;

use camino::{Utf8Component, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::{ContentHash, CoreError, CoreResult};

/// RigDeck 管理的资产种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// Agent Skill 目录或包。
    Skill,
    /// Prompt、Rule 或 instruction 片段。
    Prompt,
    /// MCP Server 定义。
    McpServer,
}

/// 资产的稳定来源身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIdentity {
    /// 来源命名空间，例如 GitHub owner 或私有源 ID。
    pub source_namespace: String,
    /// 仓库、包或目录的来源标识。
    pub package: String,
    /// 包内相对路径；仓库根资产使用 `.`。
    pub relative_path: Utf8PathBuf,
    /// 资产内容声明的名称，而不是目录名。
    pub declared_name: String,
}

impl AssetIdentity {
    /// 创建并验证资产身份。
    pub fn new(
        source_namespace: impl Into<String>,
        package: impl Into<String>,
        relative_path: impl Into<Utf8PathBuf>,
        declared_name: impl Into<String>,
    ) -> CoreResult<Self> {
        let identity = Self {
            source_namespace: source_namespace.into(),
            package: package.into(),
            relative_path: relative_path.into(),
            declared_name: declared_name.into(),
        };
        identity.validate()?;
        Ok(identity)
    }

    /// 返回由四元组内容决定的稳定 ID。
    pub fn stable_id(&self) -> String {
        let material = format!(
            "{}\0{}\0{}\0{}",
            self.source_namespace, self.package, self.relative_path, self.declared_name
        );
        ContentHash::from_bytes(material.as_bytes()).to_string()
    }

    /// 验证身份不包含路径逃逸或歧义字段。
    pub fn validate(&self) -> CoreResult<()> {
        if self.source_namespace.trim().is_empty()
            || self.package.trim().is_empty()
            || self.declared_name.trim().is_empty()
        {
            return Err(CoreError::InvalidInput(
                "资产 namespace、package 和 declared_name 均不能为空".to_owned(),
            ));
        }
        if self.declared_name.contains(['/', '\\']) {
            return Err(CoreError::InvalidInput(
                "declared_name 不能包含路径分隔符".to_owned(),
            ));
        }
        if self.relative_path.is_absolute()
            || self.relative_path.components().any(|part| {
                matches!(
                    part,
                    Utf8Component::ParentDir | Utf8Component::RootDir | Utf8Component::Prefix(_)
                )
            })
        {
            return Err(CoreError::InvalidInput(
                "资产 relative_path 必须是不能向上逃逸的相对路径".to_owned(),
            ));
        }
        Ok(())
    }
}

/// 资产来源类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// GitHub 仓库或子目录。
    Github,
    /// skills.sh 目录。
    SkillsSh,
    /// 官方或兼容 MCP Registry。
    McpRegistry,
    /// 本地文件夹。
    LocalFolder,
    /// 本地单文件，例如 Prompt 文本或规范化 MCP 定义。
    LocalFile,
    /// 本地或远端归档。
    Archive,
    /// 普通 HTTP(S) URL。
    Url,
    /// 用户配置的私有来源。
    Private,
}

/// 不含明文凭据的来源描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// 来源类型。
    pub kind: SourceKind,
    /// Provider 内的稳定命名空间。
    pub namespace: String,
    /// 可公开记录的定位符，不得携带 userinfo/token。
    pub locator: String,
    /// Provider 自身版本或 revision。
    pub revision: Option<String>,
}

/// 静态审计级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    /// 参考信息。
    Info,
    /// 需要注意但通常不阻断。
    Low,
    /// 安装前应检查。
    Medium,
    /// 默认阻断，需要高风险确认或修复。
    High,
    /// 必须阻断。
    Critical,
}

/// 一条静态审计发现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFinding {
    /// 稳定规则 ID。
    pub rule_id: String,
    /// 严重程度。
    pub severity: FindingSeverity,
    /// 中文可读说明。
    pub message: String,
    /// 可选相对文件路径。
    pub relative_path: Option<Utf8PathBuf>,
}

/// 一次资产静态审计结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditResult {
    /// 审计器协议版本。
    pub schema_version: u32,
    /// 是否完成扫描；不等于“内容绝对安全”。
    pub completed: bool,
    /// 发现列表。
    pub findings: Vec<AuditFinding>,
}

/// 系统钥匙串引用。
///
/// 该类型只保存不透明 ID。构造函数拒绝空白和明显的 `key=value` 形式，降低把
/// 明文 secret 误塞进领域对象的概率。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretRef(String);

impl SecretRef {
    /// 创建一个经过基本验证的引用 ID。
    pub fn new(value: impl Into<String>) -> CoreResult<Self> {
        let value = value.into();
        let valid = !value.trim().is_empty()
            && value.len() <= 256
            && !value.contains(['=', '\n', '\r'])
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/".contains(&byte));
        if !valid {
            return Err(CoreError::InvalidInput(
                "SecretRef 必须是不含明文值的安全标识符".to_owned(),
            ));
        }
        Ok(Self(value))
    }

    /// 借用引用 ID。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretRef(***)")
    }
}

/// MCP header/env 值：普通非敏感字面量或系统 secret 引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "binding", content = "value", rename_all = "snake_case")]
pub enum BindingValue {
    /// 明确确认不是凭据的普通值。
    Literal(String),
    /// 系统钥匙串引用。
    Secret(SecretRef),
}

/// MCP 传输方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransport {
    /// 通过子进程 stdin/stdout 通信。
    Stdio {
        /// 可执行命令。
        command: String,
        /// 命令参数。
        args: Vec<String>,
        /// 环境变量绑定。
        env: BTreeMap<String, BindingValue>,
    },
    /// Streamable HTTP 传输。
    StreamableHttp {
        /// HTTP(S) 地址。
        url: String,
        /// Header 绑定。
        headers: BTreeMap<String, BindingValue>,
    },
}

/// MCP OAuth 元数据，不包含 token。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthMetadata {
    /// OAuth issuer。
    pub issuer: String,
    /// 客户端 ID（公开标识，不是 client secret）。
    pub client_id: String,
    /// 请求 scope。
    pub scopes: Vec<String>,
}

/// 规范化 MCP Server 定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerSpec {
    /// 目标配置中的 server key。
    pub server_name: String,
    /// 传输配置。
    pub transport: McpTransport,
    /// 是否启用。
    pub enabled: bool,
    /// 请求超时毫秒数。
    pub timeout_ms: Option<u64>,
    /// 可选 OAuth 元数据。
    pub oauth: Option<OAuthMetadata>,
    /// 允许的工具名；空列表表示不额外过滤。
    pub allowed_tools: Vec<String>,
    /// 禁止的工具名。
    pub denied_tools: Vec<String>,
}

/// Prompt/Rule 的规范化元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSpec {
    /// 文本内容对象 hash。
    pub content_object: ContentHash,
    /// 多个 Prompt 的稳定排序权重。
    pub order: i32,
    /// 期望作用域。
    pub scopes: Vec<String>,
    /// 条件激活表达式；目标不支持时必须产生 compatibility loss。
    pub activation_condition: Option<String>,
}

/// Skill 的规范化元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSpec {
    /// Skill 主说明文件的相对路径。
    pub entry_path: Utf8PathBuf,
    /// 文件清单对象 hash。
    pub inventory_object: ContentHash,
    /// 原生 frontmatter 中可保留但不强制投影到其他 Agent 的字段。
    pub native_metadata: BTreeMap<String, serde_json::Value>,
}

/// 三种资产共享的版本化 spec。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec", rename_all = "snake_case")]
pub enum AssetSpec {
    /// Skill spec。
    Skill(SkillSpec),
    /// Prompt spec。
    Prompt(PromptSpec),
    /// MCP Server spec。
    McpServer(McpServerSpec),
}

impl AssetSpec {
    /// 返回 spec 对应的资产种类。
    pub fn kind(&self) -> AssetKind {
        match self {
            Self::Skill(_) => AssetKind::Skill,
            Self::Prompt(_) => AssetKind::Prompt,
            Self::McpServer(_) => AssetKind::McpServer,
        }
    }
}

/// 不可变资产修订。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRevision {
    /// 修订 ID，通常由身份与 raw hash 共同确定。
    pub id: String,
    /// 原始字节 hash。
    pub raw_hash: ContentHash,
    /// 文本语义规范化 hash。
    pub normalized_hash: ContentHash,
    /// 原始内容对象 hash。
    pub content_object: ContentHash,
    /// 来源 provenance。
    pub source: Source,
    /// SPDX 许可证表达式；未知时为 `None`。
    pub license: Option<String>,
    /// 静态审计结果。
    pub audit: AuditResult,
    /// 可安全序列化的资产 spec。
    pub spec: AssetSpec,
    /// Unix 毫秒时间戳。
    pub created_at_ms: i64,
    /// 可选作者或维护者声明；来自 provider metadata，不等于已验证身份。
    #[serde(default)]
    pub author: Option<String>,
    /// 可选来源侧更新时间（Unix 毫秒）；来自 provider metadata。
    #[serde(default)]
    pub update_time_ms: Option<i64>,
    /// 可选平台限制声明；空表示无已知限制。
    #[serde(default)]
    pub platform_restrictions: Vec<String>,
}

/// 资产生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetState {
    /// 正常可用。
    Active,
    /// 固定在当前修订。
    Pinned,
    /// 暂不参与分配。
    Disabled,
    /// 已归档但仍可恢复。
    Archived,
}

/// RigDeck 资产逻辑实体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// 稳定资产 ID。
    pub id: String,
    /// 完整来源身份。
    pub identity: AssetIdentity,
    /// 资产种类。
    pub kind: AssetKind,
    /// UI 展示名。
    pub display_name: String,
    /// 当前选中的不可变修订 ID。
    pub current_revision_id: Option<String>,
    /// 生命周期状态。
    pub state: AssetState,
    /// 用户标签。
    pub tags: Vec<String>,
}

impl Asset {
    /// 从身份创建资产，ID 由身份四元组确定。
    pub fn new(identity: AssetIdentity, kind: AssetKind) -> Self {
        let id = identity.stable_id();
        let display_name = identity.declared_name.clone();
        Self {
            id,
            identity,
            kind,
            display_name,
            current_revision_id: None,
            state: AssetState::Active,
            tags: Vec::new(),
        }
    }
}

/// Agent 实例健康状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentHealth {
    /// 检测与读取正常。
    Healthy,
    /// 权限或配置问题导致降级。
    Degraded,
    /// 适配器检测到 Agent 不存在。
    Absent,
    /// 当前环境不支持。
    Unsupported,
}

/// Agent 原生可管理表面采用的通用写入语义。
///
/// 这里描述的是“怎样保留用户内容”，而不是某个 Agent 的名字。具体路径与能力由
/// Adapter manifest 提供，所以新增 Agent 不需要修改 Core。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceMode {
    /// 目标文件完全由该资产拥有，可整体替换。
    ReplaceFile,
    /// 目标是一个 Skill 目录；资产清单决定目录内文件。
    DirectoryTree,
    /// 只管理 Markdown/文本文件中的带边界块。
    ManagedBlock,
    /// 只管理 JSONC/TOML/YAML 配置中的一个结构化条目。
    StructuredEntry,
    /// 官方没有可写本地接口，只能生成交接文件并要求人工完成。
    ManualRequired,
}

/// Adapter 检测后暴露给 Core 的一个原生管理表面。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSurface {
    /// Adapter 内稳定 surface ID。
    pub id: String,
    /// `global`、`project` 或 Adapter 声明的其他作用域。
    pub scope: String,
    /// 该表面接受的资产种类。
    pub asset_kind: AssetKind,
    /// 允许管理的绝对根目录。
    pub root_path: Utf8PathBuf,
    /// 相对根目录的目标模板；仅允许 `{name}` 变量。
    pub target_template: String,
    /// 原生格式 ID，例如 `markdown`、`jsonc` 或 `toml`。
    pub native_format: String,
    /// 结构化配置的顶层 section；其他模式为 `None`。
    pub section: Option<String>,
    /// 通用写入语义。
    pub mode: SurfaceMode,
    /// `false` 表示只兼容读取的旧路径，RigDeck 不会向这里写新内容。
    pub writable: bool,
    /// 同作用域发现多个候选时，数字越小优先级越高。
    pub precedence: u16,
}

/// 运行时检测到的 Agent 实例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInstance {
    /// 实例 ID。
    pub id: String,
    /// 运行时 adapter ID；不是 Core enum。
    pub adapter_id: String,
    /// 用户可读名称。
    pub display_name: String,
    /// 检测到的版本。
    pub version: Option<String>,
    /// 适配器允许管理的根路径。
    pub managed_roots: Vec<Utf8PathBuf>,
    /// 多 profile/instance 标识。
    pub profile: Option<String>,
    /// 当前健康状态。
    pub health: AgentHealth,
    /// Adapter 展开并验证过的原生管理表面。
    #[serde(default)]
    pub surfaces: Vec<NativeSurface>,
}

/// 将资产修订分配到 Agent 作用域的意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    /// 分配 ID。
    pub id: String,
    /// 资产 ID。
    pub asset_id: String,
    /// 修订 ID。
    pub revision_id: String,
    /// Agent 实例 ID。
    pub agent_instance_id: String,
    /// 适配器声明的作用域字符串。
    pub scope: String,
    /// 是否启用。
    pub enabled: bool,
}

/// 目标 Agent 无法完整表达的语义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityLoss {
    /// 稳定代码。
    pub code: String,
    /// 用户可读说明。
    pub message: String,
    /// 是否阻止自动应用。
    pub blocking: bool,
}

/// 适配器渲染出的单个目标文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedFile {
    /// 目标绝对路径；`DirectoryTree` 策略下是资产目录基址，最终文件路径还要安全
    /// 拼接策略内的 `relative_path`。其他策略下就是最终文件路径。
    pub target_path: Utf8PathBuf,
    /// 渲染内容所在对象库的 hash。
    pub content_object: ContentHash,
    /// 目标内容 raw hash。
    pub raw_hash: ContentHash,
    /// 原生格式标识，例如 `toml`、`jsonc`、`markdown`。
    pub native_format: String,
    /// Planner 应采用的通用投影策略。
    pub strategy: ProjectionStrategy,
}

/// 单个投影文件的保真写入策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectionStrategy {
    /// 整体替换文件。
    ReplaceFile,
    /// 按资产文件清单同步目录。
    DirectoryTree {
        /// 当前投影文件在 Skill 目录内的相对路径。
        relative_path: Utf8PathBuf,
    },
    /// 在共享文本文件中创建或替换唯一托管块。
    ManagedBlock {
        /// 不依赖展示名的稳定块 ID，通常是 Asset ID。
        block_id: String,
    },
    /// 在共享结构化配置中创建或替换一个条目。
    StructuredEntry {
        /// 顶层 section，例如 `mcpServers` 或 `mcp_servers`。
        section: String,
        /// section 内的原生 key。
        entry_key: String,
    },
}

/// 适配器投影结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    /// 资产修订 ID。
    pub revision_id: String,
    /// Agent 实例 ID。
    pub agent_instance_id: String,
    /// 目标文件列表。
    pub files: Vec<ProjectedFile>,
    /// 兼容损失。
    pub compatibility_losses: Vec<CompatibilityLoss>,
}

/// 刷新后的状态分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftState {
    /// 托管内容与基线相同。
    ManagedClean,
    /// 托管内容被 Agent/用户修改。
    ManagedModified,
    /// 新发现的外部内容。
    ExternalNew,
    /// 外部删除。
    ExternalRemoved,
    /// 来源存在新修订。
    SourceUpdateAvailable,
    /// 双边修改需要冲突处理。
    Conflict,
    /// 适配器明确不支持。
    Unsupported,
    /// 需要用户在外部界面完成。
    ManualRequired,
}

/// 冲突分类。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// 相同逻辑资产且内容相同。
    DuplicateIdentical,
    /// 同名但来源或内容不同。
    NameCollision,
    /// Agent 侧修改托管资产。
    ManagedModified,
    /// 来源更新与 Agent 侧修改同时发生。
    ConcurrentModification,
    /// 托管 Prompt block 被破坏。
    DamagedManagedBlock,
    /// 托管 Prompt block 被编辑、移动或重复。
    PromptBlockMoved,
    /// MCP server key 冲突。
    McpKeyCollision,
    /// SecretRef 缺失或无效。
    MissingSecretBinding,
    /// 目标能力损失。
    CapabilityLoss,
    /// 目录名和声明名不一致。
    DeclaredNameMismatch,
    /// 路径、大小写、symlink 或权限异常。
    PathAnomaly,
    /// 仅大小写不同的重命名。
    CaseOnlyRename,
    /// 被删除内容被 Agent 自动重建。
    RecreatedAfterRemoval,
}

/// 合法冲突动作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionAction {
    /// 使用 RigDeck 修订。
    KeepRigdeckRevision,
    /// 导入 Agent 当前修订。
    ImportAgentRevision,
    /// 保留 per-Agent 分叉。
    KeepAgentFork,
    /// 重命名后共存。
    RenameAndCoexist,
    /// 三方合并。
    ThreeWayMerge,
    /// 按文件选择。
    PerFileSelection,
    /// 放弃当前计划。
    AbandonPlan,
    /// 从备份恢复。
    RestoreBackup,
}

/// 逐文件解决时采用哪一侧内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileResolutionChoice {
    /// 使用 RigDeck 当前修订渲染的内容。
    KeepRigdeck,
    /// 使用 Agent 当前文件并采纳为基线。
    KeepAgent,
    /// 使用用户审查后的合并文本。
    Merged,
}

/// 一个受影响文件的显式选择。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResolution {
    /// 必须精确匹配冲突记录中的绝对路径。
    pub path: Utf8PathBuf,
    /// 选择的内容来源。
    pub choice: FileResolutionChoice,
    /// `merged` 时必填；服务层会立即写入加密对象库，不写入冲突或审计 JSON。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_content: Option<String>,
}

/// 生成冲突解决计划所需的完整用户请求。
///
/// 故意不派生 `Debug`，防止合并正文被错误地写入诊断日志。序列化只用于本地 IPC
/// 入参；返回的计划和审计事件永远不携带 `merged_content`。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictResolutionRequest {
    /// 解决动作。
    pub action: ResolutionAction,
    /// `rename_and_coexist` 的新声明名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_to: Option<String>,
    /// `restore_backup` 的已验证备份 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_id: Option<String>,
    /// 单文件 `three_way_merge` 的人工合并结果；自动合并成功时可省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_content: Option<String>,
    /// `per_file_selection` 的逐文件选择。
    #[serde(default)]
    pub files: Vec<FileResolution>,
}

impl From<ResolutionAction> for ConflictResolutionRequest {
    fn from(action: ResolutionAction) -> Self {
        Self {
            action,
            rename_to: None,
            backup_id: None,
            merged_content: None,
            files: Vec::new(),
        }
    }
}

/// 可解释冲突记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    /// 冲突 ID。
    pub id: String,
    /// 分类。
    pub kind: ConflictKind,
    /// 原因说明。
    pub cause: String,
    /// 受影响投影 ID/路径。
    pub affected: Vec<String>,
    /// 风险说明。
    pub risk: String,
    /// 至少一个合法下一步。
    pub actions: Vec<ResolutionAction>,
    /// 是否已经解决。
    pub resolved: bool,
    /// 发现冲突的 Agent 实例。
    #[serde(default)]
    pub agent_instance_id: Option<String>,
    /// 最近成功快照推导出的 Assignment。
    #[serde(default)]
    pub assignment_id: Option<String>,
    /// 冲突前部署基线 hash。
    #[serde(default)]
    pub baseline_hash: Option<ContentHash>,
    /// 刷新时观察到的当前 hash。
    #[serde(default)]
    pub current_hash: Option<ContentHash>,
    /// 用户选择的解决动作；只有应用关联计划后才算 resolved。
    #[serde(default)]
    pub selected_action: Option<ResolutionAction>,
    /// 关联的显式解决计划。
    #[serde(default)]
    pub resolution_plan_id: Option<String>,
    /// 成功解决时间。
    #[serde(default)]
    pub resolved_at_ms: Option<i64>,
}

/// 成功部署后的不可变基线。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentSnapshot {
    /// 快照 ID。
    pub id: String,
    /// 产生该快照的计划 ID。
    pub plan_id: String,
    /// `target path -> raw hash`。
    pub target_hashes: BTreeMap<Utf8PathBuf, ContentHash>,
    /// 本次计划验证为已删除的目标；后续重新出现时可识别 `recreated_after_removal`。
    #[serde(default)]
    pub removed_targets: Vec<Utf8PathBuf>,
    /// Unix 毫秒时间戳。
    pub created_at_ms: i64,
}

/// 不可变审计事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// 事件 ID。
    pub id: String,
    /// 事件种类。
    pub event_type: String,
    /// 关联计划 ID。
    pub plan_id: Option<String>,
    /// 不含 secret 的结构化详情。
    pub details: serde_json::Value,
    /// Unix 毫秒时间戳。
    pub created_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_uses_provenance_not_directory_name() {
        let a = AssetIdentity::new("github:a", "repo", "skills/demo", "demo").unwrap();
        let b = AssetIdentity::new("github:b", "repo", "skills/demo", "demo").unwrap();
        assert_ne!(a.stable_id(), b.stable_id());
    }

    #[test]
    fn identity_rejects_parent_traversal() {
        assert!(AssetIdentity::new("local", "fixture", "../escape", "demo").is_err());
    }

    #[test]
    fn secret_ref_debug_never_prints_identifier() {
        let reference = SecretRef::new("keychain:prod-api").unwrap();
        assert_eq!(format!("{reference:?}"), "SecretRef(***)");
    }

    #[test]
    fn json_schema_files_are_valid_json() {
        for schema_file in [
            "schema/plan.schema.json",
            "schema/conflict.schema.json",
            "schema/audit_event.schema.json",
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(schema_file);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("无法读取 {schema_file}: {error}"));
            let value: serde_json::Value = serde_json::from_str(&text)
                .unwrap_or_else(|error| panic!("{schema_file} 不是合法 JSON: {error}"));
            assert_eq!(
                value["$schema"],
                "https://json-schema.org/draft/2020-12/schema",
                "{schema_file} 缺少正确的 $schema"
            );
            assert!(value["$id"].is_string(), "{schema_file} 缺少 $id");
            assert!(value["title"].is_string(), "{schema_file} 缺少 title");
        }
    }

    #[test]
    fn content_hash_is_deterministic_property() {
        // Property: 相同输入始终产生相同 hash
        for input in [b"".as_slice(), b"a", b"hello", b"rigdeck", &vec![0xff; 100][..]] {
            let h1 = ContentHash::from_bytes(input).to_string();
            let h2 = ContentHash::from_bytes(input).to_string();
            assert_eq!(h1, h2, "ContentHash 必须对相同输入确定性");
            assert!(!h1.is_empty(), "ContentHash 不能为空");
        }
    }

    #[test]
    fn content_hash_distinguishes_different_inputs() {
        // Property: 不同输入极大概率产生不同 hash
        let inputs: &[&[u8]] = &[b"a", b"b", b"ab", b"ba", b"hello", b"world"];
        let mut hashes = std::collections::HashSet::new();
        for input in inputs {
            hashes.insert(ContentHash::from_bytes(input).to_string());
        }
        assert_eq!(hashes.len(), inputs.len(), "不同输入应产生不同 hash");
    }

    #[test]
    fn asset_identity_stable_id_is_deterministic_property() {
        // Property: 相同 provenance 始终产生相同 stable_id
        for ns in ["local", "github:acme", "skills.sh:demo"] {
            let a = AssetIdentity::new(ns, "repo", "skills/demo", "demo").unwrap();
            let b = AssetIdentity::new(ns, "repo", "skills/demo", "demo").unwrap();
            assert_eq!(a.stable_id(), b.stable_id(), "stable_id 必须确定性");
        }
    }

    #[test]
    fn asset_identity_different_provenance_different_id() {
        // Property: 不同 provenance 产生不同 stable_id
        let a = AssetIdentity::new("github:a", "repo", "skills/demo", "demo").unwrap();
        let b = AssetIdentity::new("github:b", "repo", "skills/demo", "demo").unwrap();
        assert_ne!(a.stable_id(), b.stable_id());
    }

    #[test]
    fn secret_ref_rejects_plaintext_patterns_property() {
        // Property: 明文 key=value 形式始终被拒绝
        for input in ["key=value", "token=abc123", "password=secret", "a=b"] {
            assert!(SecretRef::new(input).is_err(), "SecretRef 必须拒绝 {input}");
        }
        // Property: 合法 ID 始终被接受
        for input in ["keychain:prod-api", "vault:my-secret", "abc-123_def.ghi"] {
            assert!(SecretRef::new(input).is_ok(), "SecretRef 应接受 {input}");
        }
    }
}
