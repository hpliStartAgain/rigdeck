//! 文件级部署计划与纯规划器。

use std::{collections::BTreeSet, fs};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::{
    Asset, Assignment, CompatibilityLoss, ContentHash, ContentStore, CoreError, CoreResult,
    CORE_PROTOCOL_VERSION,
};

/// 文件操作风险级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// 只创建新文件或无损 metadata。
    Low,
    /// 覆盖 RigDeck 已托管内容。
    Medium,
    /// 删除、物化 secret 或兼容损失。
    High,
    /// 已知越界/不可恢复操作；Planner 应拒绝而不是生成此类计划。
    Critical,
}

/// 文件操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    /// 创建或原子替换文件。
    WriteFile,
    /// 删除文件。
    RemoveFile,
    /// 不修改文件，只验证并采纳当前 hash 为新的显式基线。
    AdoptBaseline,
}

/// 部署计划的业务意图。
///
/// 文件操作本身不足以判断“写入配置”是在安装、更新还是卸载共享条目；显式意图让
/// Store 能在文件事务成功后同步更新 Assignment 状态，而无需猜测。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanPurpose {
    /// 首次安装或启用。
    #[default]
    Install,
    /// 更新已有分配。
    Update,
    /// 精确卸载并禁用分配。
    Remove,
    /// 从历史状态恢复。
    Restore,
    /// 冲突解决；文件操作或基线采纳仍必须显式预览和应用。
    ConflictResolution,
}

/// 单个文件操作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedOperation {
    /// 在计划内稳定排序的操作 ID。
    pub id: String,
    /// 操作类型。
    pub kind: OperationKind,
    /// 目标绝对路径。
    pub target_path: Utf8PathBuf,
    /// 计划生成时目标 hash；`None` 表示当时文件不存在。
    pub expected_target_hash: Option<ContentHash>,
    /// 写入内容对象；删除操作为 `None`。
    pub desired_object: Option<ContentHash>,
    /// 期望应用后的 raw hash；删除操作为 `None`。
    pub desired_hash: Option<ContentHash>,
    /// 应用前目标备份对象；新建文件时为 `None`。
    pub rollback_object: Option<ContentHash>,
    /// 人类可读 diff；不得包含明文 secret。
    pub rendered_diff: String,
    /// 目标无法完整表达的语义。
    pub compatibility_losses: Vec<CompatibilityLoss>,
    /// 风险级别。
    pub risk: RiskLevel,
}

/// 文件事务成功后，与文件快照在同一数据库事务中提交的目录元数据变更。
///
/// 这里存放的只有不可变 ID 和不含密钥的模型；正文仍然只以加密对象哈希出现在计划中。
/// `enum`（枚举）表示一个值在同一时刻只能是下面某一种变更，Store 的 `match` 必须
/// 穷举所有分支，因此以后新增变更时编译器会提醒我们补齐事务实现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CatalogEffect {
    /// 把资产的当前修订切换到已经暂存的不可变修订。
    SetAssetRevision {
        /// 资产 ID。
        asset_id: String,
        /// 新修订 ID。
        revision_id: String,
    },
    /// 把既有分配切换到已经暂存的不可变修订。
    SetAssignmentRevision {
        /// 分配 ID。
        assignment_id: String,
        /// 新修订 ID。
        revision_id: String,
    },
    /// 在文件事务成功后创建或更新一个逻辑资产。
    UpsertAsset {
        /// 完整但不含正文/密钥的资产元数据。
        asset: Asset,
    },
    /// 在文件事务成功后创建或更新一个分配。
    UpsertAssignment {
        /// 完整但不含正文/密钥的分配元数据。
        assignment: Assignment,
    },
    /// 放弃一条被本冲突替代的旧计划。
    AbandonPlan {
        /// 待放弃计划 ID。
        plan_id: String,
    },
}

/// 可序列化、可失效、可审计的部署计划。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentPlan {
    /// 公共协议版本。
    pub schema_version: u32,
    /// 由计划语义内容确定的 ID。
    pub id: String,
    /// 可选分配 ID。
    pub assignment_id: Option<String>,
    /// 计划的显式业务意图；旧 schema 缺少字段时按安装读取。
    #[serde(default)]
    pub purpose: PlanPurpose,
    /// 输入资产/修订 hash；应用前必须仍可用。
    pub source_hashes: Vec<ContentHash>,
    /// 文件级操作。
    pub operations: Vec<PlannedOperation>,
    /// 文件事务成功后原子提交的目录元数据变更。
    #[serde(default)]
    pub catalog_effects: Vec<CatalogEffect>,
    /// 计划整体风险。
    pub risk: RiskLevel,
    /// Unix 毫秒时间戳。
    pub created_at_ms: i64,
}

impl DeploymentPlan {
    /// 验证计划结构与路径边界。
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != CORE_PROTOCOL_VERSION {
            return Err(CoreError::InvalidPlan(format!(
                "不支持的 schema_version：{}",
                self.schema_version
            )));
        }
        let mut targets = BTreeSet::new();
        for operation in &self.operations {
            if !operation.target_path.is_absolute() {
                return Err(CoreError::InvalidPlan(format!(
                    "目标路径必须是绝对路径：{}",
                    operation.target_path
                )));
            }
            if !targets.insert(operation.target_path.clone()) {
                return Err(CoreError::InvalidPlan(format!(
                    "同一计划不能重复修改目标：{}",
                    operation.target_path
                )));
            }
            match operation.kind {
                OperationKind::WriteFile
                    if operation.desired_object.is_none() || operation.desired_hash.is_none() =>
                {
                    return Err(CoreError::InvalidPlan(
                        "write_file 操作必须引用 desired object/hash".to_owned(),
                    ));
                }
                OperationKind::RemoveFile
                    if operation.desired_object.is_some() || operation.desired_hash.is_some() =>
                {
                    return Err(CoreError::InvalidPlan(
                        "remove_file 操作不能包含 desired object/hash".to_owned(),
                    ));
                }
                OperationKind::AdoptBaseline
                    if operation.desired_object.is_some()
                        || operation.rollback_object.is_some()
                        || operation.expected_target_hash.is_none()
                        || operation.desired_hash != operation.expected_target_hash =>
                {
                    return Err(CoreError::InvalidPlan(
                        "adopt_baseline 必须采纳已存在文件的当前 hash，且不能引用写入/回滚对象"
                            .to_owned(),
                    ));
                }
                _ => {}
            }
            if operation.kind != OperationKind::AdoptBaseline
                && operation.expected_target_hash.is_some()
                && operation.rollback_object.is_none()
            {
                return Err(CoreError::InvalidPlan(format!(
                    "覆盖/删除前必须有 rollback object：{}",
                    operation.target_path
                )));
            }
        }
        for effect in &self.catalog_effects {
            match effect {
                CatalogEffect::SetAssetRevision {
                    asset_id,
                    revision_id,
                } if asset_id.is_empty() || revision_id.is_empty() => {
                    return Err(CoreError::InvalidPlan(
                        "set_asset_revision 的资产/修订 ID 不能为空".to_owned(),
                    ));
                }
                CatalogEffect::SetAssignmentRevision {
                    assignment_id,
                    revision_id,
                } if assignment_id.is_empty() || revision_id.is_empty() => {
                    return Err(CoreError::InvalidPlan(
                        "set_assignment_revision 的分配/修订 ID 不能为空".to_owned(),
                    ));
                }
                CatalogEffect::UpsertAsset { asset } => {
                    asset.identity.validate()?;
                    if asset.id != asset.identity.stable_id() {
                        return Err(CoreError::InvalidPlan(
                            "upsert_asset 的 ID 与稳定身份不一致".to_owned(),
                        ));
                    }
                }
                CatalogEffect::UpsertAssignment { assignment }
                    if assignment.id.is_empty()
                        || assignment.asset_id.is_empty()
                        || assignment.revision_id.is_empty()
                        || assignment.agent_instance_id.is_empty()
                        || assignment.scope.trim().is_empty() =>
                {
                    return Err(CoreError::InvalidPlan(
                        "upsert_assignment 含空 ID/scope".to_owned(),
                    ));
                }
                CatalogEffect::AbandonPlan { plan_id } if plan_id.is_empty() => {
                    return Err(CoreError::InvalidPlan(
                        "abandon_plan 的计划 ID 不能为空".to_owned(),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// 增量构造部署计划的 Planner。
pub struct Planner<'a> {
    assignment_id: Option<String>,
    purpose: PlanPurpose,
    source_hashes: Vec<ContentHash>,
    operations: Vec<PlannedOperation>,
    catalog_effects: Vec<CatalogEffect>,
    objects: &'a dyn ContentStore,
}

impl<'a> Planner<'a> {
    /// 创建 Planner。
    ///
    /// 生命周期参数 `'a` 表示 Planner 不能比借用的对象库活得更久；Rust 编译器会
    /// 阻止悬空引用，而无需运行时检查。
    pub fn new(
        assignment_id: Option<String>,
        source_hashes: Vec<ContentHash>,
        objects: &'a dyn ContentStore,
    ) -> Self {
        Self {
            assignment_id,
            purpose: PlanPurpose::Install,
            source_hashes,
            operations: Vec::new(),
            catalog_effects: Vec::new(),
            objects,
        }
    }

    /// 设置计划意图，通常在添加文件操作前调用。
    pub fn with_purpose(mut self, purpose: PlanPurpose) -> Self {
        self.purpose = purpose;
        self
    }

    /// 在计算确定性计划 ID 前统一清理人类可读 diff。
    ///
    /// 闭包只接触展示文本，不能改变目标路径、对象 hash 或回滚语义。Host 可在这里
    /// 接入钥匙串感知的脱敏器，而 Core 无需反向依赖安全实现 crate。
    pub fn sanitize_rendered_diffs<F>(&mut self, mut sanitize: F)
    where
        F: FnMut(&str) -> String,
    {
        for operation in &mut self.operations {
            operation.rendered_diff = sanitize(&operation.rendered_diff);
        }
    }

    /// 登记一个只有在文件事务成功后才提交的目录元数据变更。
    pub fn add_catalog_effect(&mut self, effect: CatalogEffect) {
        self.catalog_effects.push(effect);
    }

    /// 规划创建或替换文件；目标已是相同内容时不生成操作。
    pub fn write_file(
        &mut self,
        target_path: Utf8PathBuf,
        desired_bytes: &[u8],
        rendered_diff: impl Into<String>,
        compatibility_losses: Vec<CompatibilityLoss>,
        risk: RiskLevel,
    ) -> CoreResult<()> {
        validate_target(&target_path)?;
        let desired_hash = ContentHash::from_bytes(desired_bytes);
        let current = read_current(&target_path)?;
        let expected_target_hash = current.as_deref().map(ContentHash::from_bytes);
        if expected_target_hash.as_ref() == Some(&desired_hash) {
            return Ok(());
        }

        let desired_object = self.objects.put(desired_bytes)?;
        let rollback_object = current
            .as_deref()
            .map(|bytes| self.objects.put(bytes))
            .transpose()?;
        let operation = PlannedOperation {
            id: operation_id(OperationKind::WriteFile, &target_path, Some(&desired_hash)),
            kind: OperationKind::WriteFile,
            target_path,
            expected_target_hash,
            desired_object: Some(desired_object),
            desired_hash: Some(desired_hash),
            rollback_object,
            rendered_diff: rendered_diff.into(),
            compatibility_losses,
            risk,
        };
        self.operations.push(operation);
        Ok(())
    }

    /// 规划删除文件；目标不存在时不生成操作。
    pub fn remove_file(
        &mut self,
        target_path: Utf8PathBuf,
        rendered_diff: impl Into<String>,
        risk: RiskLevel,
    ) -> CoreResult<()> {
        validate_target(&target_path)?;
        let Some(current) = read_current(&target_path)? else {
            return Ok(());
        };
        let expected = ContentHash::from_bytes(&current);
        let rollback = self.objects.put(&current)?;
        self.operations.push(PlannedOperation {
            id: operation_id(OperationKind::RemoveFile, &target_path, None),
            kind: OperationKind::RemoveFile,
            target_path,
            expected_target_hash: Some(expected),
            desired_object: None,
            desired_hash: None,
            rollback_object: Some(rollback),
            rendered_diff: rendered_diff.into(),
            compatibility_losses: Vec::new(),
            risk,
        });
        Ok(())
    }

    /// 规划“保留 Agent 当前文件”为新基线；应用阶段只校验 hash，不改写文件。
    pub fn adopt_baseline(
        &mut self,
        target_path: Utf8PathBuf,
        rendered_diff: impl Into<String>,
    ) -> CoreResult<()> {
        validate_target(&target_path)?;
        let current = read_current(&target_path)?.ok_or_else(|| {
            CoreError::InvalidPlan(format!("无法采纳不存在的文件：{target_path}"))
        })?;
        let hash = ContentHash::from_bytes(&current);
        self.operations.push(PlannedOperation {
            id: operation_id(OperationKind::AdoptBaseline, &target_path, Some(&hash)),
            kind: OperationKind::AdoptBaseline,
            target_path,
            expected_target_hash: Some(hash.clone()),
            desired_object: None,
            desired_hash: Some(hash),
            rollback_object: None,
            rendered_diff: rendered_diff.into(),
            compatibility_losses: Vec::new(),
            risk: RiskLevel::Medium,
        });
        Ok(())
    }

    /// 完成计划并计算确定性 ID。
    pub fn finish(mut self, created_at_ms: i64) -> CoreResult<DeploymentPlan> {
        self.operations
            .sort_by(|left, right| left.target_path.cmp(&right.target_path));
        let risk = self
            .operations
            .iter()
            .map(|operation| operation.risk)
            .max()
            .unwrap_or(RiskLevel::Low);
        let identity_material = serde_json::to_vec(&(
            CORE_PROTOCOL_VERSION,
            &self.assignment_id,
            self.purpose,
            &self.source_hashes,
            &self.operations,
            &self.catalog_effects,
        ))?;
        let id = ContentHash::from_bytes(&identity_material).to_string();
        let plan = DeploymentPlan {
            schema_version: CORE_PROTOCOL_VERSION,
            id,
            assignment_id: self.assignment_id,
            purpose: self.purpose,
            source_hashes: self.source_hashes,
            operations: self.operations,
            catalog_effects: self.catalog_effects,
            risk,
            created_at_ms,
        };
        plan.validate()?;
        Ok(plan)
    }
}

fn validate_target(path: &Utf8PathBuf) -> CoreResult<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, camino::Utf8Component::ParentDir))
    {
        return Err(CoreError::InvalidInput(format!(
            "Planner 目标必须是无 `..` 的绝对路径：{path}"
        )));
    }
    Ok(())
}

fn read_current(path: &Utf8PathBuf) -> CoreResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CoreError::io(path, error)),
    }
}

fn operation_id(
    kind: OperationKind,
    target: &Utf8PathBuf,
    desired: Option<&ContentHash>,
) -> String {
    let material = format!(
        "{kind:?}\0{target}\0{}",
        desired.map_or("", ContentHash::as_str)
    );
    ContentHash::from_bytes(material.as_bytes()).to_string()
}
