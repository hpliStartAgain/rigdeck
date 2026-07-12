//! RigDeck 共享应用服务。
//!
//! CLI 与桌面 IPC 都只调用本 crate。这里负责把 Adapter、Registry、Store 与 Core
//! Planner 组合起来；前端和命令行不会直接接触 SQLite 或 Agent 文件。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod backup;
mod bundle;
mod error;
mod paths;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use camino::{Utf8Path, Utf8PathBuf};
use rigdeck_adapter_sdk::{AgentAdapter, AssetContent, AssetFileContent, DetectionContext};
use rigdeck_adapters::{
    add_projection_to_planner, add_removals_to_planner, persist_rendered_objects, BuiltinAdapter,
    RefreshCoordinator, StartupRefreshOutcome, WatchService,
};
use rigdeck_core::{
    merge_json, merge_text, normalized_hash, AgentInstance, ApplyReport, Asset, AssetIdentity,
    AssetKind, AssetRevision, AssetSpec, Assignment, AuditResult, BaselineFile, BindingValue,
    CatalogEffect, ConflictResolutionRequest, ContentHash, DeploymentPlan, FileResolution,
    FileResolutionChoice, McpServerSpec, McpTransport, MergeResult, OperationKind, PlanPurpose,
    Planner, ProjectionStrategy, PromptSpec, ResolutionAction, SecretRef, Source, SourceKind,
    TransactionEngine,
};
use rigdeck_registry::{
    extract_archive, import_skill_bundle, persist_imported_skill, read_local_directory,
    strip_common_root, CatalogProvider, FetchedAsset, FetchedCatalogAsset, FileHttpCache,
    GithubProvider, ImportedSkill, McpRegistryProvider, SearchResult, SkillsShProvider,
    UrlProvider,
};
use rigdeck_security::{NativeSecretVault, SecretValue, SecretVault};
use rigdeck_store::{Database, ObjectKey, ObjectStore};
use serde::Serialize;

pub use backup::*;
pub use bundle::*;
pub use error::*;
pub use paths::*;

const OBJECT_KEY_REF: &str = "keychain:rigdeck-object-store-v1";
const STALE_PLAN_AFTER_MS: i64 = 24 * 60 * 60 * 1_000;
const MAX_SINGLE_ASSET_BYTES: u64 = 1024 * 1024;

/// Overview/CLI status 的共享结构。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceStatus {
    /// 公共 JSON schema 版本。
    pub schema_version: u32,
    /// 已检测 Agent。
    pub agents: Vec<AgentInstance>,
    /// 资产数量。
    pub asset_count: usize,
    /// 分配数量。
    pub assignment_count: usize,
    /// 未解决冲突数量。
    pub conflict_count: usize,
}

/// 一次本地 Skill 导入的展示结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetInspection {
    /// 资产。
    pub asset: Asset,
    /// 修订。
    pub revision: rigdeck_core::AssetRevision,
    /// 安装前可审查的文件清单。
    pub files: Vec<AssetFileSummary>,
}

/// 桌面端本地资产分页结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetPage {
    /// 当前页资产。
    pub items: Vec<Asset>,
    /// 符合筛选条件的总数。
    pub total: usize,
    /// 当前页起始偏移。
    pub offset: usize,
    /// 实际采用的页大小。
    pub limit: usize,
}

/// 资产修订中的一个文件摘要；不包含可能很大的正文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetFileSummary {
    /// Bundle 内相对路径。
    pub path: Utf8PathBuf,
    /// 原始内容 hash。
    pub hash: ContentHash,
    /// 字节长度。
    pub size: u64,
    /// 来源是否声明为可执行。
    pub executable: bool,
}

/// 一次来源更新检查。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateReport {
    /// 已检查资产数。
    pub checked: usize,
    /// 发现并持久化的新修订。
    pub updated: Vec<AssetInspection>,
    /// 为已有启用分配生成的更新计划。
    pub plans: Vec<DeploymentPlan>,
    /// 暂不支持自动更新的来源说明。
    pub skipped: Vec<String>,
}

/// 旧 Registry 一次性导入结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyImportReport {
    /// 成功导入的资产 ID。
    pub imported: Vec<String>,
    /// 跳过的目录及原因。
    pub skipped: Vec<String>,
}

/// Doctor 单项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorCheck {
    /// 稳定检查 ID。
    pub id: String,
    /// 是否通过。
    pub healthy: bool,
    /// 中文说明。
    pub message: String,
}

/// Doctor 报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    /// 总体是否健康。
    pub healthy: bool,
    /// 检查项。
    pub checks: Vec<DoctorCheck>,
}

/// watcher 捕获变化后触发的一次本地全量校准结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WatchRefreshOutcome {
    /// 去重后的变化路径。
    pub changed_paths: Vec<Utf8PathBuf>,
    /// create/modify/remove/other 变化种类。
    pub kinds: Vec<String>,
    /// 重新扫描后的可信状态。
    pub refresh: StartupRefreshOutcome,
}

/// CLI/Tauri 共用的有状态应用服务。
pub struct RigDeckService {
    paths: AppPaths,
    database: Database,
    objects: ObjectStore,
    http_cache: Arc<FileHttpCache>,
    vault: Arc<dyn SecretVault>,
    refresher: RefreshCoordinator,
    watcher: Option<WatchService>,
    refresh_context: Option<DetectionContext>,
    /// 后台工作取消标志；`true` 表示已请求取消。
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
}

/// 可脱离有状态 `RigDeckService` 锁运行的只读远端目录服务。
#[derive(Debug, Clone)]
pub struct CatalogService {
    cache: Arc<FileHttpCache>,
}

impl CatalogService {
    /// 搜索 skills.sh。
    pub async fn search_skills(&self, query: &str, limit: usize) -> ServiceResult<SearchResult> {
        let provider = SkillsShProvider::official(self.cache.as_ref(), None)?;
        Ok(provider.search(query, limit).await?)
    }

    /// 搜索 Official MCP Registry preview。
    pub async fn search_mcp(&self, query: &str, limit: usize) -> ServiceResult<SearchResult> {
        let provider = McpRegistryProvider::official_preview(self.cache.as_ref())?;
        Ok(provider.search(query, limit).await?)
    }

    /// 获取 Official MCP Registry 的标准 server metadata，不执行其中的包。
    pub async fn fetch_mcp(&self, id: &str) -> ServiceResult<FetchedCatalogAsset> {
        let provider = McpRegistryProvider::official_preview(self.cache.as_ref())?;
        Ok(provider.fetch(id).await?)
    }
}

impl std::fmt::Debug for RigDeckService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RigDeckService")
            .field("paths", &self.paths)
            .field("database", &self.database)
            .field("objects", &self.objects)
            .field("http_cache", &self.http_cache)
            .field("watcher_active", &self.watcher.is_some())
            .finish_non_exhaustive()
    }
}

impl RigDeckService {
    /// 使用系统钥匙串打开默认生产服务。
    pub fn open(paths: AppPaths) -> ServiceResult<Self> {
        let vault: Arc<dyn SecretVault> = Arc::new(NativeSecretVault::new());
        let key = load_or_create_object_key(vault.as_ref())?;
        Self::open_with_key(paths, key, vault)
    }

    /// 使用显式对象密钥打开服务；测试不依赖真实系统钥匙串。
    pub fn open_with_key(
        paths: AppPaths,
        key: ObjectKey,
        vault: Arc<dyn SecretVault>,
    ) -> ServiceResult<Self> {
        fs::create_dir_all(&paths.data_root)
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        let database = Database::open(paths.database.clone())?;
        let objects = ObjectStore::open(paths.object_store.clone(), key)?;
        let http_cache = Arc::new(FileHttpCache::open(paths.http_cache.clone())?);
        let refresher = RefreshCoordinator::with_builtins()?;
        Ok(Self {
            paths,
            database,
            objects,
            http_cache,
            vault,
            refresher,
            watcher: None,
            refresh_context: None,
            cancel_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// 应用路径。
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// 请求取消正在运行的后台工作（如 `update_assets`）。
    /// 取消是协作式的：长循环在每个资产边界检查标志并提前返回。
    pub fn request_cancel(&self) {
        self.cancel_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// 清除取消标志，使后续后台工作可以正常运行。
    pub fn clear_cancel(&self) {
        self.cancel_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// 返回取消标志的当前状态。
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// 公开 HTTP cache，供异步 Registry provider 复用。
    pub fn http_cache(&self) -> &FileHttpCache {
        self.http_cache.as_ref()
    }

    /// 克隆只读 Catalog 句柄；它不持有数据库、Planner 或 watcher 锁。
    pub fn catalog(&self) -> CatalogService {
        CatalogService {
            cache: Arc::clone(&self.http_cache),
        }
    }

    /// 当前本地状态。
    pub fn status(&self) -> ServiceResult<ServiceStatus> {
        Ok(ServiceStatus {
            schema_version: 1,
            agents: self.database.list_agent_instances()?,
            asset_count: self.database.count_assets()?,
            assignment_count: self.database.list_assignments()?.len(),
            conflict_count: self.database.list_conflicts(Some(false))?.len(),
        })
    }

    /// 列出资产。
    pub fn list_assets(&self) -> ServiceResult<Vec<Asset>> {
        Ok(self.database.list_assets()?)
    }

    /// 分页检索本地资产。所有边界在服务层统一校验，Desktop 不直接拼 SQL。
    pub fn list_assets_page(
        &self,
        kind: Option<AssetKind>,
        query: Option<&str>,
        source_namespace: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> ServiceResult<AssetPage> {
        if offset > 1_000_000 {
            return Err(ServiceError::InvalidInput(
                "资产分页 offset 不能超过 1,000,000".to_owned(),
            ));
        }
        if !(1..=200).contains(&limit) {
            return Err(ServiceError::InvalidInput(
                "资产分页 limit 必须在 1..=200".to_owned(),
            ));
        }
        let query = query.unwrap_or("").trim();
        let source_namespace = source_namespace.unwrap_or("").trim();
        if query.len() > 200 || source_namespace.len() > 200 {
            return Err(ServiceError::InvalidInput(
                "资产筛选文本不能超过 200 字节".to_owned(),
            ));
        }
        let kind = kind.map(|kind| match kind {
            AssetKind::Skill => "skill",
            AssetKind::Prompt => "prompt",
            AssetKind::McpServer => "mcp_server",
        });
        let (items, total) =
            self.database
                .list_assets_page(kind, query, source_namespace, offset, limit)?;
        Ok(AssetPage {
            items,
            total,
            offset,
            limit,
        })
    }

    /// 列出资产到 Agent/scope 的全部分配。
    pub fn list_assignments(&self) -> ServiceResult<Vec<Assignment>> {
        Ok(self.database.list_assignments()?)
    }

    /// 列出计划。
    pub fn list_plans(&self, status: Option<&str>) -> ServiceResult<Vec<DeploymentPlan>> {
        Ok(self.database.list_plans(status)?)
    }

    /// 列出冲突。
    pub fn list_conflicts(
        &self,
        resolved: Option<bool>,
    ) -> ServiceResult<Vec<rigdeck_core::Conflict>> {
        Ok(self.database.list_conflicts(resolved)?)
    }

    /// 读取冲突。
    pub fn conflict(&self, id: &str) -> ServiceResult<rigdeck_core::Conflict> {
        self.database
            .load_conflict(id)?
            .ok_or_else(|| ServiceError::NotFound(format!("冲突 {id}")))
    }

    /// 为受支持的冲突动作生成显式计划；只有计划应用成功后才标记 resolved。
    pub fn resolve_conflict(
        &mut self,
        id: &str,
        action: ResolutionAction,
    ) -> ServiceResult<DeploymentPlan> {
        self.resolve_conflict_request(id, action.into())
    }

    /// 根据带参数的请求生成冲突解决计划。请求中的合并正文只会写入加密对象库，
    /// 返回计划、冲突记录和审计事件均只保存内容哈希。
    pub fn resolve_conflict_request(
        &mut self,
        id: &str,
        request: ConflictResolutionRequest,
    ) -> ServiceResult<DeploymentPlan> {
        let conflict = self.conflict(id)?;
        let action = request.action.clone();
        if !conflict.actions.contains(&action) {
            return Err(ServiceError::InvalidInput(format!(
                "动作 {action:?} 不是冲突 {id} 的合法下一步"
            )));
        }
        let plan = match action.clone() {
            ResolutionAction::KeepRigdeckRevision => {
                let assignment = self.conflict_assignment(&conflict)?;
                self.plan_rigdeck_side(&assignment)?
            }
            ResolutionAction::ImportAgentRevision => {
                let assignment = self.conflict_assignment(&conflict)?;
                let revision = self.stage_agent_revision(&assignment, &conflict.affected)?;
                let mut planner = self.agent_baseline_planner(&conflict, &assignment)?;
                planner.add_catalog_effect(CatalogEffect::SetAssetRevision {
                    asset_id: assignment.asset_id.clone(),
                    revision_id: revision.id.clone(),
                });
                planner.add_catalog_effect(CatalogEffect::SetAssignmentRevision {
                    assignment_id: assignment.id,
                    revision_id: revision.id,
                });
                self.finish_resolution_plan(planner)?
            }
            ResolutionAction::KeepAgentFork => {
                let assignment = self.conflict_assignment(&conflict)?;
                let planner = self.agent_baseline_planner(&conflict, &assignment)?;
                self.finish_resolution_plan(planner)?
            }
            ResolutionAction::RenameAndCoexist => {
                let name = request.rename_to.as_deref().ok_or_else(|| {
                    ServiceError::InvalidInput("重命名共存必须提供 rename_to".to_owned())
                })?;
                self.plan_rename_and_coexist(&conflict, name)?
            }
            ResolutionAction::ThreeWayMerge => {
                self.plan_three_way_merge(&conflict, request.merged_content.as_deref())?
            }
            ResolutionAction::PerFileSelection => {
                self.plan_per_file_selection(&conflict, &request.files)?
            }
            ResolutionAction::AbandonPlan => {
                let marker = self.objects.put_bytes(conflict.id.as_bytes())?;
                let planner = Planner::new(None, vec![marker], &self.objects)
                    .with_purpose(PlanPurpose::ConflictResolution);
                self.finish_resolution_plan(planner)?
            }
            ResolutionAction::RestoreBackup => {
                let backup_id = request.backup_id.as_deref().ok_or_else(|| {
                    ServiceError::InvalidInput("从备份恢复必须提供 backup_id".to_owned())
                })?;
                self.plan_conflict_backup_restore(&conflict, backup_id)?
            }
        };
        self.database
            .attach_conflict_plan(id, action, &plan.id, now_ms())?;
        Ok(plan)
    }

    fn conflict_assignment(&self, conflict: &rigdeck_core::Conflict) -> ServiceResult<Assignment> {
        let assignment_id = conflict.assignment_id.as_deref().ok_or_else(|| {
            ServiceError::ManualRequired(
                "冲突缺少 DeploymentSnapshot/Assignment 上下文，需先 refresh/doctor".to_owned(),
            )
        })?;
        self.database
            .list_assignments()?
            .into_iter()
            .find(|assignment| assignment.id == assignment_id)
            .ok_or_else(|| ServiceError::NotFound(format!("分配 {assignment_id}")))
    }

    /// 重新渲染 RigDeck 当前修订，但把业务意图固定为冲突解决。
    fn plan_rigdeck_side(&self, assignment: &Assignment) -> ServiceResult<DeploymentPlan> {
        let plan = self.build_rigdeck_side(assignment)?;
        self.database.save_plan(&plan)?;
        Ok(plan)
    }

    /// 构建但不持久化 RigDeck 侧计划，供合并/逐文件选择复用其期望内容。
    fn build_rigdeck_side(&self, assignment: &Assignment) -> ServiceResult<DeploymentPlan> {
        let inspection = self.inspect(&assignment.asset_id)?;
        let instance = self
            .database
            .load_agent_instance(&assignment.agent_instance_id)?
            .ok_or_else(|| {
                ServiceError::NotFound(format!("Agent 实例 {}", assignment.agent_instance_id))
            })?;
        let adapter = BuiltinAdapter::load(&instance.adapter_id)?;
        let content = self.load_revision_content(&inspection.revision)?;
        let secret_refs = secret_refs(&inspection.revision);
        let secret_values = secret_refs
            .iter()
            .map(|reference| self.vault.get(reference))
            .collect::<Result<Vec<_>, _>>()?;
        let rendered = adapter.render_bundle(
            &inspection.asset,
            &inspection.revision,
            &content,
            &instance,
            &assignment.scope,
            &secret_refs,
        )?;
        persist_rendered_objects(&rendered, &self.objects)?;
        let source_hashes = content_object_hashes(&content, &inspection.revision)?;
        let mut planner = Planner::new(Some(assignment.id.clone()), source_hashes, &self.objects)
            .with_purpose(PlanPurpose::ConflictResolution);
        add_projection_to_planner(&mut planner, &instance, &rendered.projection, &self.objects)?;
        planner.sanitize_rendered_diffs(|diff| rigdeck_security::redact_text(diff, &secret_values));
        Ok(planner.finish(now_ms())?)
    }

    fn finish_resolution_plan(&self, planner: Planner<'_>) -> ServiceResult<DeploymentPlan> {
        let plan = planner.finish(now_ms())?;
        self.database.save_plan(&plan)?;
        Ok(plan)
    }

    fn validate_conflict_file(
        &self,
        conflict: &rigdeck_core::Conflict,
        path: &Utf8Path,
    ) -> ServiceResult<Vec<u8>> {
        if !conflict.affected.iter().any(|value| value == path.as_str()) {
            return Err(ServiceError::InvalidInput(format!(
                "路径不属于冲突受影响集合：{path}"
            )));
        }
        let bytes =
            fs::read(path).map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        if conflict.affected.len() == 1
            && conflict.current_hash.as_ref() != Some(&ContentHash::from_bytes(&bytes))
        {
            return Err(ServiceError::InvalidInput(
                "冲突发现后文件又发生变化，请先重新 refresh".to_owned(),
            ));
        }
        Ok(bytes)
    }

    fn agent_baseline_planner(
        &self,
        conflict: &rigdeck_core::Conflict,
        assignment: &Assignment,
    ) -> ServiceResult<Planner<'_>> {
        let mut planner = Planner::new(Some(assignment.id.clone()), Vec::new(), &self.objects)
            .with_purpose(PlanPurpose::ConflictResolution);
        for affected in &conflict.affected {
            let path = Utf8PathBuf::from(affected);
            self.validate_conflict_file(conflict, &path)?;
            planner.adopt_baseline(path, "采纳 Agent 当前文件为新的显式基线")?;
        }
        Ok(planner)
    }

    fn plan_three_way_merge(
        &self,
        conflict: &rigdeck_core::Conflict,
        reviewed: Option<&str>,
    ) -> ServiceResult<DeploymentPlan> {
        let [affected] = conflict.affected.as_slice() else {
            return Err(ServiceError::InvalidInput(
                "三方合并当前要求冲突精确对应一个文件；多文件请使用逐文件选择".to_owned(),
            ));
        };
        let assignment = self.conflict_assignment(conflict)?;
        let rigdeck = self.build_rigdeck_side(&assignment)?;
        let path = Utf8PathBuf::from(affected);
        let theirs = self.validate_conflict_file(conflict, &path)?;
        let base_hash = conflict
            .baseline_hash
            .as_ref()
            .ok_or_else(|| ServiceError::ManualRequired("冲突缺少可验证的部署基线".to_owned()))?;
        let base = self.objects.get_bytes(base_hash)?;
        let ours = desired_bytes_for(&rigdeck, &path, &self.objects)?;
        let merged = if let Some(reviewed) = reviewed {
            validate_resolution_text(reviewed)?
        } else {
            automatic_merge(&path, &base, &ours, &theirs)?
        };
        let mut planner = Planner::new(Some(assignment.id), vec![base_hash.clone()], &self.objects)
            .with_purpose(PlanPurpose::ConflictResolution);
        if merged == theirs {
            planner.adopt_baseline(path, "三方合并结果等于 Agent 当前文件，采纳为新基线")?;
        } else {
            planner.write_file(
                path,
                &merged,
                "写入已审查的三方合并结果",
                Vec::new(),
                rigdeck_core::RiskLevel::High,
            )?;
        }
        self.finish_resolution_plan(planner)
    }

    fn plan_per_file_selection(
        &self,
        conflict: &rigdeck_core::Conflict,
        selections: &[FileResolution],
    ) -> ServiceResult<DeploymentPlan> {
        let assignment = self.conflict_assignment(conflict)?;
        let rigdeck = self.build_rigdeck_side(&assignment)?;
        let selected: BTreeMap<_, _> = selections
            .iter()
            .map(|selection| (selection.path.as_str(), selection))
            .collect();
        if selected.len() != selections.len()
            || selected.len() != conflict.affected.len()
            || conflict
                .affected
                .iter()
                .any(|path| !selected.contains_key(path.as_str()))
        {
            return Err(ServiceError::InvalidInput(
                "逐文件选择必须无重复且完整覆盖 affected 路径".to_owned(),
            ));
        }
        let mut planner = Planner::new(Some(assignment.id), Vec::new(), &self.objects)
            .with_purpose(PlanPurpose::ConflictResolution);
        for affected in &conflict.affected {
            let path = Utf8PathBuf::from(affected);
            let choice = selected[affected.as_str()];
            match choice.choice {
                FileResolutionChoice::KeepRigdeck => {
                    add_selected_rigdeck_operation(&mut planner, &rigdeck, &path, &self.objects)?;
                }
                FileResolutionChoice::KeepAgent => {
                    self.validate_conflict_file(conflict, &path)?;
                    planner.adopt_baseline(path, "逐文件选择：保留 Agent 当前文件")?;
                }
                FileResolutionChoice::Merged => {
                    let merged = choice.merged_content.as_deref().ok_or_else(|| {
                        ServiceError::InvalidInput(format!("merged 选择缺少正文：{path}"))
                    })?;
                    let merged = validate_resolution_text(merged)?;
                    let current = self.validate_conflict_file(conflict, &path)?;
                    if current == merged {
                        planner.adopt_baseline(
                            path,
                            "逐文件合并结果等于 Agent 当前文件，采纳为新基线",
                        )?;
                    } else {
                        planner.write_file(
                            path,
                            &merged,
                            "逐文件选择：写入审查后的合并结果",
                            Vec::new(),
                            rigdeck_core::RiskLevel::High,
                        )?;
                    }
                }
            }
        }
        self.finish_resolution_plan(planner)
    }

    /// 从可逆的原生投影中重建资产内容，并暂存一条不可变修订。
    ///
    /// ManagedBlock/StructuredEntry 需要 codec 提供反向解析才能保证不把块外用户内容
    /// 误导入资产，因此此处明确拒绝；refresh 会在后续迭代中据此收窄可用动作。
    fn stage_agent_revision(
        &self,
        assignment: &Assignment,
        affected: &[String],
    ) -> ServiceResult<AssetRevision> {
        let inspection = self.inspect(&assignment.asset_id)?;
        let mut content = self.load_revision_content(&inspection.revision)?;
        let instance = self
            .database
            .load_agent_instance(&assignment.agent_instance_id)?
            .ok_or_else(|| {
                ServiceError::NotFound(format!("Agent 实例 {}", assignment.agent_instance_id))
            })?;
        let adapter = BuiltinAdapter::load(&instance.adapter_id)?;
        let rendered = adapter.render_bundle(
            &inspection.asset,
            &inspection.revision,
            &content,
            &instance,
            &assignment.scope,
            &secret_refs(&inspection.revision),
        )?;
        let mut replaced = BTreeSet::new();
        for projected in &rendered.projection.files {
            let (target, source_path) = match &projected.strategy {
                ProjectionStrategy::DirectoryTree { relative_path } => (
                    projected.target_path.join(relative_path),
                    relative_path.clone(),
                ),
                ProjectionStrategy::ReplaceFile => {
                    let [source] = content.files.as_slice() else {
                        return Err(ServiceError::InvalidInput(
                            "replace_file 资产必须只有一个内容文件".to_owned(),
                        ));
                    };
                    (projected.target_path.clone(), source.relative_path.clone())
                }
                ProjectionStrategy::ManagedBlock { .. }
                | ProjectionStrategy::StructuredEntry { .. } => {
                    return Err(ServiceError::ManualRequired(
                        "当前原生表面需要保真反向 codec，不能安全导入整份共享配置".to_owned(),
                    ));
                }
            };
            if affected.iter().any(|path| path == target.as_str()) {
                let bytes = fs::read(&target)
                    .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
                let source = content
                    .files
                    .iter_mut()
                    .find(|file| file.relative_path == source_path)
                    .ok_or_else(|| {
                        ServiceError::InvalidInput(format!("投影缺少源文件：{source_path}"))
                    })?;
                source.bytes = bytes;
                replaced.insert(target.to_string());
            }
        }
        if replaced.len() != affected.len() {
            return Err(ServiceError::InvalidInput(
                "冲突路径不能完整映射回资产内容，请先 refresh/doctor".to_owned(),
            ));
        }
        self.persist_derived_revision(
            &inspection.asset,
            &inspection.revision,
            &content,
            affected.first().map(String::as_str).unwrap_or("agent"),
        )
    }

    fn persist_derived_revision(
        &self,
        asset: &Asset,
        base: &AssetRevision,
        content: &AssetContent,
        locator: &str,
    ) -> ServiceResult<AssetRevision> {
        if content.files.is_empty() {
            return Err(ServiceError::InvalidInput(
                "导入修订没有内容文件".to_owned(),
            ));
        }
        for file in &content.files {
            self.objects.put_bytes(&file.bytes)?;
        }
        let (raw_hash, normalized, content_object, spec) = match &base.spec {
            AssetSpec::Skill(skill) => {
                let inventory = content.inventory_bytes()?;
                let inventory_object = self.objects.put_bytes(&inventory)?;
                let entry = content
                    .files
                    .iter()
                    .find(|file| file.relative_path == skill.entry_path)
                    .ok_or_else(|| {
                        ServiceError::InvalidInput("Skill 修订缺少入口文件".to_owned())
                    })?;
                let entry_hash = ContentHash::from_bytes(&entry.bytes);
                let mut skill = skill.clone();
                skill.inventory_object = inventory_object;
                (
                    content.raw_hash()?,
                    content.normalized_hash()?,
                    entry_hash.clone(),
                    AssetSpec::Skill(skill),
                )
            }
            AssetSpec::Prompt(prompt) => {
                let [file] = content.files.as_slice() else {
                    return Err(ServiceError::InvalidInput(
                        "Prompt 修订必须只有一个内容文件".to_owned(),
                    ));
                };
                std::str::from_utf8(&file.bytes).map_err(|_| {
                    ServiceError::InvalidInput("Prompt 修订必须是 UTF-8".to_owned())
                })?;
                let hash = ContentHash::from_bytes(&file.bytes);
                let mut prompt = prompt.clone();
                prompt.content_object = hash.clone();
                (
                    hash.clone(),
                    normalized_hash(&file.bytes),
                    hash,
                    AssetSpec::Prompt(prompt),
                )
            }
            AssetSpec::McpServer(_) => {
                return Err(ServiceError::ManualRequired(
                    "MCP 共享结构必须通过原生 codec 反向解析，不能按整文件导入".to_owned(),
                ));
            }
        };
        let revision = AssetRevision {
            id: ContentHash::from_bytes(format!("revision\0{}\0{raw_hash}", asset.id).as_bytes())
                .to_string(),
            raw_hash,
            normalized_hash: normalized,
            content_object,
            source: Source {
                kind: SourceKind::LocalFile,
                namespace: "agent-import".to_owned(),
                locator: locator.to_owned(),
                revision: Some(ContentHash::from_bytes(locator.as_bytes()).to_string()),
            },
            license: base.license.clone(),
            audit: AuditResult {
                schema_version: 1,
                completed: true,
                findings: Vec::new(),
            },
            spec,
            created_at_ms: now_ms(),
            author: None,
            update_time_ms: None,
            platform_restrictions: Vec::new(),
        };
        self.database.save_revision(&revision)?;
        Ok(revision)
    }

    fn plan_rename_and_coexist(
        &self,
        conflict: &rigdeck_core::Conflict,
        rename_to: &str,
    ) -> ServiceResult<DeploymentPlan> {
        let assignment = self.conflict_assignment(conflict)?;
        let original = self.inspect(&assignment.asset_id)?;
        let identity = AssetIdentity::new(
            format!("agent:{}", assignment.agent_instance_id),
            &original.asset.identity.package,
            &original.asset.identity.relative_path,
            rename_to.trim(),
        )?;
        let mut fork = Asset::new(identity, original.asset.kind);
        let mut fork_assignment = assignment.clone();
        fork_assignment.asset_id = fork.id.clone();
        fork_assignment.id = ContentHash::from_bytes(
            format!(
                "assignment\0{}\0{}\0{}",
                fork.id, fork_assignment.agent_instance_id, fork_assignment.scope
            )
            .as_bytes(),
        )
        .to_string();

        // 先用原资产的可逆投影重建 Agent 内容，再让新资产拥有这条修订。
        let staged = self.stage_agent_revision(&assignment, &conflict.affected)?;
        let content = self.load_revision_content(&staged)?;
        let fork_revision = self.persist_derived_revision(
            &fork,
            &staged,
            &content,
            conflict
                .affected
                .first()
                .map(String::as_str)
                .unwrap_or("agent"),
        )?;
        fork.current_revision_id = Some(fork_revision.id.clone());
        fork_assignment.revision_id = fork_revision.id.clone();
        fork_assignment.enabled = true;

        let instance = self
            .database
            .load_agent_instance(&assignment.agent_instance_id)?
            .ok_or_else(|| ServiceError::NotFound("Agent 实例不存在".to_owned()))?;
        let adapter = BuiltinAdapter::load(&instance.adapter_id)?;
        let rendered = adapter.render_bundle(
            &fork,
            &fork_revision,
            &content,
            &instance,
            &assignment.scope,
            &secret_refs(&fork_revision),
        )?;
        persist_rendered_objects(&rendered, &self.objects)?;
        let mut planner = Planner::new(
            Some(assignment.id.clone()),
            content_object_hashes(&content, &fork_revision)?,
            &self.objects,
        )
        .with_purpose(PlanPurpose::ConflictResolution);
        // 原名恢复 RigDeck 版本，新名写入 Agent 分叉，两份内容因此可同时存在。
        let original_plan = self.build_rigdeck_side(&assignment)?;
        for affected in &conflict.affected {
            add_selected_rigdeck_operation(
                &mut planner,
                &original_plan,
                &Utf8PathBuf::from(affected),
                &self.objects,
            )?;
        }
        add_projection_to_planner(&mut planner, &instance, &rendered.projection, &self.objects)?;
        planner.add_catalog_effect(CatalogEffect::UpsertAsset { asset: fork });
        planner.add_catalog_effect(CatalogEffect::UpsertAssignment {
            assignment: fork_assignment,
        });
        self.finish_resolution_plan(planner)
    }

    fn plan_conflict_backup_restore(
        &self,
        conflict: &rigdeck_core::Conflict,
        backup_id: &str,
    ) -> ServiceResult<DeploymentPlan> {
        let backup = self.open_backup_for_plan(backup_id)?;
        let mut latest = BTreeMap::<Utf8PathBuf, Option<ContentHash>>::new();
        for snapshot in backup.list_snapshots()? {
            for path in snapshot.removed_targets {
                latest.insert(path, None);
            }
            for (path, hash) in snapshot.target_hashes {
                latest.insert(path, Some(hash));
            }
        }
        let mut planner = Planner::new(conflict.assignment_id.clone(), Vec::new(), &self.objects)
            .with_purpose(PlanPurpose::Restore);
        for affected in &conflict.affected {
            let path = Utf8PathBuf::from(affected);
            match latest.get(&path) {
                Some(Some(hash)) => {
                    let bytes = self.objects.get_bytes(hash)?;
                    planner.write_file(
                        path,
                        &bytes,
                        format!("从已验证备份 {backup_id} 恢复文件"),
                        Vec::new(),
                        rigdeck_core::RiskLevel::High,
                    )?;
                }
                Some(None) => planner.remove_file(
                    path,
                    format!("按已验证备份 {backup_id} 恢复为不存在状态"),
                    rigdeck_core::RiskLevel::High,
                )?,
                None => {
                    return Err(ServiceError::NotFound(format!(
                        "备份 {backup_id} 没有路径 {path} 的部署基线"
                    )));
                }
            }
        }
        self.finish_resolution_plan(planner)
    }

    /// 最近审计事件。
    pub fn activity(&self, limit: usize) -> ServiceResult<Vec<rigdeck_core::AuditEvent>> {
        Ok(self.database.list_audit_events(limit)?)
    }

    /// 把资产固定在当前修订；已固定的资产再次固定是幂等的。
    pub fn pin_asset(&self, asset_id: &str) -> ServiceResult<Asset> {
        let mut asset = self
            .database
            .load_asset(asset_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("资产 {asset_id}")))?;
        if asset.state == rigdeck_core::AssetState::Archived {
            return Err(ServiceError::InvalidInput(
                "已归档资产必须先恢复才能固定".to_owned(),
            ));
        }
        asset.state = rigdeck_core::AssetState::Pinned;
        self.database.save_asset(&asset, now_ms())?;
        self.audit_asset_change("asset_pinned", asset_id)?;
        Ok(asset)
    }

    /// 解除固定，回到 Active 状态。
    pub fn unpin_asset(&self, asset_id: &str) -> ServiceResult<Asset> {
        let mut asset = self
            .database
            .load_asset(asset_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("资产 {asset_id}")))?;
        if asset.state != rigdeck_core::AssetState::Pinned {
            return Err(ServiceError::InvalidInput(format!(
                "资产 {asset_id} 当前状态为 {:?}，不是 Pinned",
                asset.state
            )));
        }
        asset.state = rigdeck_core::AssetState::Active;
        self.database.save_asset(&asset, now_ms())?;
        self.audit_asset_change("asset_unpinned", asset_id)?;
        Ok(asset)
    }

    /// 归档资产；归档后不参与新分配，但保留修订和备份，可恢复。
    pub fn archive_asset(&self, asset_id: &str) -> ServiceResult<Asset> {
        let mut asset = self
            .database
            .load_asset(asset_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("资产 {asset_id}")))?;
        if asset.state == rigdeck_core::AssetState::Archived {
            return Ok(asset);
        }
        asset.state = rigdeck_core::AssetState::Archived;
        self.database.save_asset(&asset, now_ms())?;
        self.audit_asset_change("asset_archived", asset_id)?;
        Ok(asset)
    }

    /// 从归档恢复资产到 Active 状态。
    pub fn restore_asset(&self, asset_id: &str) -> ServiceResult<Asset> {
        let mut asset = self
            .database
            .load_asset(asset_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("资产 {asset_id}")))?;
        if asset.state != rigdeck_core::AssetState::Archived {
            return Err(ServiceError::InvalidInput(format!(
                "资产 {asset_id} 当前状态为 {:?}，不是 Archived",
                asset.state
            )));
        }
        asset.state = rigdeck_core::AssetState::Active;
        self.database.save_asset(&asset, now_ms())?;
        self.audit_asset_change("asset_restored", asset_id)?;
        Ok(asset)
    }

    fn audit_asset_change(&self, event_type: &str, asset_id: &str) -> ServiceResult<()> {
        let created_at_ms = now_ms();
        let event = rigdeck_core::AuditEvent {
            id: ContentHash::from_bytes(
                format!("audit\0{event_type}\0{asset_id}\0{created_at_ms}").as_bytes(),
            )
            .to_string(),
            event_type: event_type.to_owned(),
            plan_id: None,
            details: serde_json::json!({ "asset_id": asset_id }),
            created_at_ms,
        };
        self.database.save_audit_event(&event)?;
        Ok(())
    }

    /// 把 secret 写入系统钥匙串；审计只记录 SecretRef，绝不记录值。
    pub fn set_secret(&self, reference: &str, bytes: Vec<u8>) -> ServiceResult<SecretRef> {
        if bytes.is_empty() || bytes.len() > 64 * 1024 {
            return Err(ServiceError::InvalidInput(
                "secret 必须为 1 到 65536 字节".to_owned(),
            ));
        }
        let reference = SecretRef::new(reference)?;
        let previous = match self.vault.get(&reference) {
            Ok(value) => Some(value),
            Err(rigdeck_security::SecretError::NotFound(_)) => None,
            Err(error) => return Err(error.into()),
        };
        self.vault.put(&reference, &SecretValue::new(bytes))?;
        if let Err(error) = self.audit_secret_change("secret_binding_saved", &reference) {
            // `previous` 拥有旧 SecretValue；离开作用域时其内存会由 Drop 主动清零。
            match previous {
                Some(value) => self.vault.put(&reference, &value)?,
                None => self.vault.delete(&reference)?,
            }
            return Err(error);
        }
        Ok(reference)
    }

    /// 删除钥匙串中的 secret；不存在时保持幂等。
    pub fn delete_secret(&self, reference: &str) -> ServiceResult<SecretRef> {
        let reference = SecretRef::new(reference)?;
        let previous = match self.vault.get(&reference) {
            Ok(value) => Some(value),
            Err(rigdeck_security::SecretError::NotFound(_)) => None,
            Err(error) => return Err(error.into()),
        };
        self.vault.delete(&reference)?;
        if let Err(error) = self.audit_secret_change("secret_binding_deleted", &reference) {
            if let Some(value) = previous {
                self.vault.put(&reference, &value)?;
            }
            return Err(error);
        }
        Ok(reference)
    }

    /// 只返回 SecretRef 是否存在，不读取或序列化 secret 值。
    pub fn has_secret(&self, reference: &str) -> ServiceResult<bool> {
        let reference = SecretRef::new(reference)?;
        match self.vault.get(&reference) {
            Ok(_) => Ok(true),
            Err(rigdeck_security::SecretError::NotFound(_)) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// 每次启动或手工请求时检测并刷新全部本地 Agent，然后再启动 watcher。
    pub fn refresh(
        &mut self,
        home: Utf8PathBuf,
        project_root: Option<Utf8PathBuf>,
    ) -> ServiceResult<StartupRefreshOutcome> {
        let baselines = self.baselines_by_instance()?;
        let context = DetectionContext { home, project_root };
        let outcome = self.refresher.run(&context, &baselines, &BTreeMap::new());
        for result in &outcome.instances {
            self.database
                .save_agent_instance(&result.instance, result.report.finished_at_ms)?;
            self.database
                .commit_refresh(&result.instance.id, &result.report, &result.observed)?;
        }
        self.watcher = if outcome.watch_roots.is_empty() {
            None
        } else {
            Some(outcome.start_watcher(Duration::from_millis(250))?)
        };
        self.refresh_context = Some(context);
        Ok(outcome)
    }

    /// 非阻塞或短等待地读取 watcher 批次；有变化时执行一次全量本地校准。
    ///
    /// watcher 事件只作为“缓存可能失效”的提示，不能直接推断最终状态。重新扫描可以
    /// 合并操作系统丢失/乱序事件，并继续复用启动刷新相同的 hash 与冲突分类逻辑。
    pub fn poll_watcher(
        &mut self,
        timeout: Duration,
    ) -> ServiceResult<Option<WatchRefreshOutcome>> {
        if timeout > Duration::from_millis(250) {
            return Err(ServiceError::InvalidInput(
                "单次 watcher 轮询不能阻塞超过 250ms".to_owned(),
            ));
        }
        let Some(watcher) = self.watcher.as_ref() else {
            return Ok(None);
        };
        let Some(batch) = watcher.next_batch(timeout)? else {
            return Ok(None);
        };
        let context = self
            .refresh_context
            .clone()
            .ok_or_else(|| ServiceError::InvalidInput("watcher 缺少最近刷新上下文".to_owned()))?;
        let refresh = self.refresh(context.home, context.project_root)?;
        Ok(Some(WatchRefreshOutcome {
            changed_paths: batch.paths,
            kinds: batch.kinds.into_iter().collect(),
            refresh,
        }))
    }

    /// 导入并持久化一个本地多文件 Skill。
    pub fn add_local_skill(&self, root: &Utf8Path) -> ServiceResult<AssetInspection> {
        let content = read_local_directory(root)?;
        let package = root.file_name().unwrap_or("local-skill").to_owned();
        let source = rigdeck_core::Source {
            kind: rigdeck_core::SourceKind::LocalFolder,
            namespace: "local".to_owned(),
            locator: root.to_string(),
            revision: None,
        };
        let imported = import_skill_bundle(content, source, "local", package, ".", None, now_ms())?;
        self.persist_import(imported)
    }

    /// 从旧 agent-skill-registry 一次性导入 Skill。
    ///
    /// 扫描 `skills/` 下的两级目录（category/name），对每个含 SKILL.md 的目录
    /// 调用 `add_local_skill`。manifests 不读取——旧 Registry 的安装视图只作为
    /// 候选，不直接标记为已托管。导入后用户自行 assign。
    pub fn import_legacy_registry(
        &self,
        registry_root: &Utf8Path,
    ) -> ServiceResult<LegacyImportReport> {
        let skills_dir = registry_root.join("skills");
        if !skills_dir.is_dir() {
            return Err(ServiceError::InvalidInput(format!(
                "{registry_root} 下没有 skills/ 目录，不是有效的旧 Registry"
            )));
        }
        let mut imported = Vec::new();
        let mut skipped = Vec::new();
        // 遍历 skills/<category>/<name>/SKILL.md
        for category_entry in fs::read_dir(&skills_dir)
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?
        {
            let category_path = Utf8PathBuf::from_path_buf(
                category_entry
                    .map_err(|error| ServiceError::InvalidInput(error.to_string()))?
                    .path(),
            )
            .map_err(|_| ServiceError::InvalidInput("路径不是 UTF-8".to_owned()))?;
            if !category_path.is_dir() {
                continue;
            }
            for skill_entry in fs::read_dir(&category_path)
                .map_err(|error| ServiceError::InvalidInput(error.to_string()))?
            {
                let skill_path = Utf8PathBuf::from_path_buf(
                    skill_entry
                        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?
                        .path(),
                )
                .map_err(|_| ServiceError::InvalidInput("路径不是 UTF-8".to_owned()))?;
                if !skill_path.is_dir() {
                    continue;
                }
                let skill_md = skill_path.join("SKILL.md");
                if !skill_md.is_file() {
                    skipped.push(format!(
                        "{}：无 SKILL.md",
                        skill_path.file_name().unwrap_or("?")
                    ));
                    continue;
                }
                match self.add_local_skill(&skill_path) {
                    Ok(inspection) => {
                        imported.push(inspection.asset.id);
                    }
                    Err(error) => {
                        skipped.push(format!(
                            "{}：{}",
                            skill_path.file_name().unwrap_or("?"),
                            error
                        ));
                    }
                }
            }
        }
        Ok(LegacyImportReport { imported, skipped })
    }

    /// 导入本地 UTF-8 Prompt/Rule/Instruction 文件。
    pub fn add_local_prompt(
        &self,
        path: &Utf8Path,
        declared_name: Option<&str>,
        scopes: &[String],
    ) -> ServiceResult<AssetInspection> {
        let bytes = read_safe_single_file(path)?;
        std::str::from_utf8(&bytes)
            .map_err(|_| ServiceError::InvalidInput("Prompt 必须是 UTF-8 文本".to_owned()))?;
        let name = declared_name
            .map(str::to_owned)
            .or_else(|| path.file_stem().map(str::to_owned))
            .ok_or_else(|| ServiceError::InvalidInput("无法从文件名确定 Prompt 名称".to_owned()))?;
        let scopes = if scopes.is_empty() {
            vec!["global".to_owned(), "project".to_owned()]
        } else {
            scopes.to_vec()
        };
        if scopes
            .iter()
            .any(|scope| !matches!(scope.as_str(), "global" | "project" | "repository"))
        {
            return Err(ServiceError::InvalidInput(
                "Prompt scope 只能是 global、project 或 repository".to_owned(),
            ));
        }
        let content_hash = ContentHash::from_bytes(&bytes);
        self.persist_single_asset(
            path,
            name,
            AssetKind::Prompt,
            bytes,
            AssetSpec::Prompt(PromptSpec {
                content_object: content_hash,
                order: 0,
                scopes,
                activation_condition: None,
            }),
        )
    }

    /// 导入本地规范化 MCP Server JSON；敏感绑定必须使用 `SecretRef`。
    pub fn add_local_mcp(&self, path: &Utf8Path) -> ServiceResult<AssetInspection> {
        let source_bytes = read_safe_single_file(path)?;
        let spec: McpServerSpec = serde_json::from_slice(&source_bytes)?;
        validate_mcp_spec(&spec)?;
        // 只持久化经过反序列化和安全校验的规范化表示，避免把未知字段或原始凭据
        // 原封不动写入加密对象库。加密并不等于允许保存明文 secret。
        let canonical = serde_json::to_vec(&spec)?;
        self.persist_single_asset(
            path,
            spec.server_name.clone(),
            AssetKind::McpServer,
            canonical,
            AssetSpec::McpServer(spec),
        )
    }

    /// 从 Official MCP Registry 导入唯一且无需额外 header 的 Streamable HTTP remote。
    pub async fn add_mcp_registry(
        &self,
        id: &str,
        alias: Option<&str>,
    ) -> ServiceResult<AssetInspection> {
        let fetched = self.catalog().fetch_mcp(id).await?;
        self.add_fetched_mcp(fetched, alias)
    }

    /// 持久化已获取的 MCP metadata；供桌面端在远端请求结束后短暂加锁调用。
    pub fn add_fetched_mcp(
        &self,
        fetched: FetchedCatalogAsset,
        alias: Option<&str>,
    ) -> ServiceResult<AssetInspection> {
        let FetchedCatalogAsset::McpMetadata { item, server_json } = fetched else {
            return Err(ServiceError::InvalidInput(
                "远端对象不是 MCP Registry metadata".to_owned(),
            ));
        };
        let remotes = server_json
            .get("remotes")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                ServiceError::ManualRequired(
                    "该 MCP 条目只有 package 安装方式；必须由用户选择并审核运行时命令".to_owned(),
                )
            })?;
        if remotes.len() != 1 {
            return Err(ServiceError::ManualRequired(format!(
                "该 MCP 条目提供 {} 个 remote；必须显式选择目标",
                remotes.len()
            )));
        }
        let remote = &remotes[0];
        if remote.get("type").and_then(serde_json::Value::as_str) != Some("streamable-http") {
            return Err(ServiceError::ManualRequired(
                "当前仅能自动导入 streamable-http remote".to_owned(),
            ));
        }
        if remote
            .get("headers")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|headers| !headers.is_empty())
        {
            return Err(ServiceError::ManualRequired(
                "该 remote 声明了 header；请先创建 SecretRef 并使用本地规范化 MCP JSON 导入"
                    .to_owned(),
            ));
        }
        let remote_url = remote
            .get("url")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ServiceError::InvalidInput("MCP remote 缺少 URL".to_owned()))?;
        let server_name = alias.map(str::to_owned).unwrap_or_else(|| {
            item.name
                .rsplit('/')
                .next()
                .unwrap_or(&item.name)
                .to_owned()
        });
        let spec = McpServerSpec {
            server_name: server_name.clone(),
            transport: McpTransport::StreamableHttp {
                url: remote_url.to_owned(),
                headers: BTreeMap::new(),
            },
            enabled: true,
            timeout_ms: Some(30_000),
            oauth: None,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
        };
        validate_mcp_spec(&spec)?;
        let canonical = serde_json::to_vec(&spec)?;
        let source = Source {
            kind: SourceKind::McpRegistry,
            namespace: item.provider_id.clone(),
            locator: item.locator,
            revision: item.version,
        };
        self.persist_single_asset_from_source(
            &item.provider_id,
            &item.id,
            ".",
            source,
            server_name,
            AssetKind::McpServer,
            canonical,
            AssetSpec::McpServer(spec),
        )
    }

    /// 搜索 skills.sh。远端不可用不会影响任何本地方法。
    pub async fn search_skills(&self, query: &str, limit: usize) -> ServiceResult<SearchResult> {
        self.catalog().search_skills(query, limit).await
    }

    /// 搜索 Official MCP Registry preview。
    ///
    /// MCP 官方建议 Host 优先接兼容的下游 Registry，因此桌面设置页后续可把同一
    /// provider 合同换成组织内地址；CLI 的默认值明确标记为 preview。
    pub async fn search_mcp(&self, query: &str, limit: usize) -> ServiceResult<SearchResult> {
        self.catalog().search_mcp(query, limit).await
    }

    /// 从本地目录、本地归档、skills.sh ID 或 GitHub HTTPS URL 导入 Skill。
    ///
    /// Rust 提示：`async fn` 返回的是 Future；只有调用方 `.await` 时网络请求才真正
    /// 推进。本地分支虽然不需要等待，但保留统一接口能让 CLI 与 Tauri 共用同一路径。
    pub async fn add_skill_source(&self, source: &str) -> ServiceResult<AssetInspection> {
        let local = Utf8Path::new(source);
        if local.is_dir() {
            return self.add_local_skill(local);
        }
        if local.is_file() {
            let metadata = fs::symlink_metadata(local)
                .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
            if metadata.file_type().is_symlink() || metadata.len() > 64 * 1024 * 1024 {
                return Err(ServiceError::InvalidInput(
                    "本地归档必须是真实普通文件且不超过 64 MiB".to_owned(),
                ));
            }
            let bytes =
                fs::read(local).map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
            let content = strip_common_root(extract_archive(&bytes)?)?;
            let package = local.file_stem().unwrap_or("local-archive").to_owned();
            let imported = import_skill_bundle(
                content,
                rigdeck_core::Source {
                    kind: rigdeck_core::SourceKind::Archive,
                    namespace: "local".to_owned(),
                    locator: local.to_string(),
                    revision: Some(ContentHash::from_bytes(&bytes).to_string()),
                },
                "local",
                package,
                ".",
                None,
                now_ms(),
            )?;
            return self.persist_import(imported);
        }

        if let Some(id) = source.strip_prefix("skills.sh:") {
            let provider = SkillsShProvider::official(self.http_cache.as_ref(), None)?;
            return self.persist_fetched_skill(provider.fetch(id).await?);
        }
        if source.starts_with("https://github.com/") {
            let provider = GithubProvider::new(self.http_cache.as_ref(), None)?;
            return self.persist_fetched_skill(provider.fetch(source).await?);
        }
        if let Some(id) = source.strip_prefix("https://skills.sh/") {
            let provider = SkillsShProvider::official(self.http_cache.as_ref(), None)?;
            return self.persist_fetched_skill(provider.fetch(id.trim_matches('/')).await?);
        }
        if source.starts_with("https://") {
            let provider = UrlProvider::new(self.http_cache.as_ref(), None)?;
            return self.persist_fetched_skill(provider.fetch(source).await?);
        }

        Err(ServiceError::InvalidInput(
            "Skill 来源必须是本地目录/归档、skills.sh:<id>、skills.sh URL、GitHub HTTPS URL 或通用 HTTPS URL"
                .to_owned(),
        ))
    }

    /// 检查全部 Skill 来源，持久化新修订并为已启用分配生成更新计划。
    ///
    /// 这个方法不会应用计划；远端失败会作为显式错误返回，旧修订和本地部署不受影响。
    pub async fn update_assets(&self) -> ServiceResult<UpdateReport> {
        let assets = self.database.list_assets()?;
        let checked = assets.len();
        let mut updated = Vec::new();
        let mut plans = Vec::new();
        let mut skipped = Vec::new();

        for asset in assets {
            if self.is_cancelled() {
                skipped.push(format!("{}：用户取消，未检查", asset.id));
                break;
            }
            let current = self.inspect(&asset.id)?;
            if current.asset.kind != rigdeck_core::AssetKind::Skill {
                skipped.push(format!("{}：当前仅自动更新 Skill", asset.id));
                continue;
            }
            let imported = match &current.revision.source.kind {
                rigdeck_core::SourceKind::LocalFolder => {
                    let root = Utf8Path::new(&current.revision.source.locator);
                    let content = read_local_directory(root)?;
                    import_skill_bundle(
                        content,
                        current.revision.source.clone(),
                        current.asset.identity.source_namespace.clone(),
                        current.asset.identity.package.clone(),
                        current.asset.identity.relative_path.clone(),
                        current.revision.license.clone(),
                        now_ms(),
                    )?
                }
                rigdeck_core::SourceKind::Archive => {
                    let path = Utf8Path::new(&current.revision.source.locator);
                    let bytes = fs::read(path)
                        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
                    let mut source = current.revision.source.clone();
                    source.revision = Some(ContentHash::from_bytes(&bytes).to_string());
                    import_skill_bundle(
                        strip_common_root(extract_archive(&bytes)?)?,
                        source,
                        current.asset.identity.source_namespace.clone(),
                        current.asset.identity.package.clone(),
                        current.asset.identity.relative_path.clone(),
                        current.revision.license.clone(),
                        now_ms(),
                    )?
                }
                rigdeck_core::SourceKind::SkillsSh => {
                    let id = current
                        .revision
                        .source
                        .locator
                        .strip_prefix("https://skills.sh/")
                        .ok_or_else(|| {
                            ServiceError::InvalidInput("skills.sh locator 无效".to_owned())
                        })?;
                    let provider = SkillsShProvider::official(self.http_cache.as_ref(), None)?;
                    imported_from_fetched(provider.fetch(id.trim_matches('/')).await?)?
                }
                rigdeck_core::SourceKind::Github => {
                    let provider = GithubProvider::new(self.http_cache.as_ref(), None)?;
                    imported_from_fetched(provider.fetch(&current.revision.source.locator).await?)?
                }
                rigdeck_core::SourceKind::Url => {
                    let provider = UrlProvider::new(self.http_cache.as_ref(), None)?;
                    imported_from_fetched(provider.fetch(&current.revision.source.locator).await?)?
                }
                other => {
                    skipped.push(format!("{}：来源 {other:?} 需要专用 provider", asset.id));
                    continue;
                }
            };
            if imported.asset.id != current.asset.id {
                return Err(ServiceError::InvalidInput(format!(
                    "来源更新改变了资产身份：{} -> {}",
                    current.asset.id, imported.asset.id
                )));
            }
            if imported.revision.raw_hash == current.revision.raw_hash {
                continue;
            }
            let inspection = self.persist_import(imported)?;
            for assignment in self
                .database
                .list_assignments()?
                .into_iter()
                .filter(|assignment| assignment.asset_id == asset.id && assignment.enabled)
            {
                plans.push(self.plan_assignment(
                    &asset.id,
                    &assignment.agent_instance_id,
                    &assignment.scope,
                )?);
            }
            updated.push(inspection);
        }
        Ok(UpdateReport {
            checked,
            updated,
            plans,
            skipped,
        })
    }

    /// 从数据库读取资产和当前修订。
    pub fn inspect(&self, asset_id: &str) -> ServiceResult<AssetInspection> {
        let asset = self
            .database
            .load_asset(asset_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("资产 {asset_id}")))?;
        let revision_id = asset
            .current_revision_id
            .as_deref()
            .ok_or_else(|| ServiceError::NotFound(format!("资产 {asset_id} 没有当前修订")))?;
        let revision = self
            .database
            .load_revision(revision_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("修订 {revision_id}")))?;
        let content = self.load_revision_content(&revision)?;
        Ok(AssetInspection {
            asset,
            revision,
            files: file_summaries(&content),
        })
    }

    /// 为一次资产分配生成并持久化显式计划，不修改 Agent 文件。
    pub fn plan_assignment(
        &self,
        asset_id: &str,
        agent_instance_id: &str,
        scope: &str,
    ) -> ServiceResult<DeploymentPlan> {
        let inspection = self.inspect(asset_id)?;
        let instance = self
            .database
            .load_agent_instance(agent_instance_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("Agent 实例 {agent_instance_id}")))?;
        let adapter = BuiltinAdapter::load(&instance.adapter_id)?;
        let content = self.load_revision_content(&inspection.revision)?;
        let secret_refs = secret_refs(&inspection.revision);
        let secret_values = secret_refs
            .iter()
            .map(|reference| self.vault.get(reference))
            .collect::<Result<Vec<_>, _>>()?;
        let rendered = adapter.render_bundle(
            &inspection.asset,
            &inspection.revision,
            &content,
            &instance,
            scope,
            &secret_refs,
        )?;
        persist_rendered_objects(&rendered, &self.objects)?;
        let assignment_id = ContentHash::from_bytes(
            format!("assignment\0{asset_id}\0{agent_instance_id}\0{scope}").as_bytes(),
        )
        .to_string();
        let existing = self
            .database
            .list_assignments()?
            .into_iter()
            .find(|assignment| assignment.id == assignment_id);
        let purpose = if existing.is_some() {
            PlanPurpose::Update
        } else {
            PlanPurpose::Install
        };
        let assignment = Assignment {
            id: assignment_id.clone(),
            asset_id: asset_id.to_owned(),
            revision_id: inspection.revision.id.clone(),
            agent_instance_id: agent_instance_id.to_owned(),
            scope: scope.to_owned(),
            // 新分配只有文件事务提交后才算启用；已有分配在准备更新计划时保持现状。
            enabled: existing.as_ref().is_some_and(|value| value.enabled),
        };
        self.database.save_assignment(&assignment, now_ms())?;
        let source_hashes = content_object_hashes(&content, &inspection.revision)?;
        let mut planner =
            Planner::new(Some(assignment_id), source_hashes, &self.objects).with_purpose(purpose);
        add_projection_to_planner(&mut planner, &instance, &rendered.projection, &self.objects)?;
        planner.sanitize_rendered_diffs(|diff| rigdeck_security::redact_text(diff, &secret_values));
        let plan = planner.finish(now_ms())?;
        self.database.save_plan(&plan)?;
        Ok(plan)
    }

    /// 为一个资产的全部已启用分配生成精确卸载计划。
    ///
    /// 目录型 Skill 只会删除成功快照证明属于该分配的文件；未知文件不进入计划。
    pub fn plan_remove_asset(&self, asset_id: &str) -> ServiceResult<Vec<DeploymentPlan>> {
        let inspection = self.inspect(asset_id)?;
        let content = self.load_revision_content(&inspection.revision)?;
        let source_hashes = content_object_hashes(&content, &inspection.revision)?;
        let assignments: Vec<_> = self
            .database
            .list_assignments()?
            .into_iter()
            .filter(|assignment| assignment.asset_id == asset_id && assignment.enabled)
            .collect();
        if assignments.is_empty() {
            return Err(ServiceError::NotFound(format!(
                "资产 {asset_id} 没有已启用分配"
            )));
        }

        let mut plans = Vec::new();
        for assignment in assignments {
            plans.push(self.plan_remove_assignment(
                &inspection,
                &assignment,
                source_hashes.clone(),
            )?);
        }
        Ok(plans)
    }

    /// 为一个既有分配生成启用或停用计划；不会直接修改 Agent 文件。
    pub fn plan_assignment_enabled(
        &self,
        assignment_id: &str,
        enabled: bool,
    ) -> ServiceResult<DeploymentPlan> {
        let assignment = self
            .database
            .list_assignments()?
            .into_iter()
            .find(|assignment| assignment.id == assignment_id)
            .ok_or_else(|| ServiceError::NotFound(format!("分配 {assignment_id}")))?;
        if enabled {
            return self.plan_assignment(
                &assignment.asset_id,
                &assignment.agent_instance_id,
                &assignment.scope,
            );
        }
        let inspection = self.inspect(&assignment.asset_id)?;
        let content = self.load_revision_content(&inspection.revision)?;
        let source_hashes = content_object_hashes(&content, &inspection.revision)?;
        self.plan_remove_assignment(&inspection, &assignment, source_hashes)
    }

    /// 应用已保存计划；阻断型兼容损失必须先通过其他专用流程解决。
    pub fn apply_plan(&mut self, plan_id: &str) -> ServiceResult<ApplyReport> {
        let plan = self
            .database
            .load_plan(plan_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("计划 {plan_id}")))?;
        let status = self
            .database
            .plan_status(plan_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("计划状态 {plan_id}")))?;
        if status != "pending" {
            return Err(ServiceError::InvalidInput(format!(
                "计划 {plan_id} 当前状态为 {status}，只能应用 pending 计划"
            )));
        }
        let blocking: Vec<_> = plan
            .operations
            .iter()
            .flat_map(|operation| &operation.compatibility_losses)
            .filter(|loss| loss.blocking)
            .map(|loss| loss.message.clone())
            .collect();
        if !blocking.is_empty() {
            return Err(ServiceError::ManualRequired(blocking.join("；")));
        }
        let objects = &self.objects;
        let database = &mut self.database;
        let report =
            TransactionEngine.apply(&plan, objects, &rigdeck_core::NoFailure, |report| {
                database
                    .commit_apply(&plan, report, now_ms())
                    .map(|_| ())
                    .map_err(|error| rigdeck_core::CoreError::CommitFailed(error.to_string()))
            })?;
        Ok(report)
    }

    /// 运行数据库、对象库、钥匙串、Adapter、Agent 路径、陈旧计划与备份检查。
    pub fn doctor(&self) -> DoctorReport {
        let checks = vec![
            check_result("database", self.database.integrity_check()),
            check_result("object_store", self.objects.integrity_check()),
            check_result("keychain", self.check_keychain()),
            check_result("adapters", BuiltinAdapter::load_all()),
            check_result("agent_paths", self.check_agent_paths()),
            check_result("stale_plans", self.check_stale_plans()),
            check_result("recoverable_backups", self.check_backups()),
        ];
        DoctorReport {
            healthy: checks.iter().all(|check| check.healthy),
            checks,
        }
    }

    fn check_keychain(&self) -> ServiceResult<()> {
        let reference = SecretRef::new(OBJECT_KEY_REF)?;
        let value = self.vault.get(&reference)?;
        if value.expose().len() != 32 {
            return Err(ServiceError::InvalidInput(
                "钥匙串中的对象库密钥长度不是 32 字节".to_owned(),
            ));
        }
        Ok(())
    }

    fn check_agent_paths(&self) -> ServiceResult<()> {
        let instances = self.database.list_agent_instances()?;
        for instance in instances {
            for root in instance.managed_roots {
                let metadata = fs::symlink_metadata(&root).map_err(|error| {
                    ServiceError::InvalidInput(format!("Agent 路径不可访问：{root}：{error}"))
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(ServiceError::InvalidInput(format!(
                        "Agent 管理根必须是真实目录且不能是 symlink：{root}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn check_stale_plans(&self) -> ServiceResult<()> {
        let now = now_ms();
        let stale = self
            .database
            .list_plans(Some("pending"))?
            .into_iter()
            .filter_map(|plan| {
                let too_old = now.saturating_sub(plan.created_at_ms) > STALE_PLAN_AFTER_MS;
                let invalid = TransactionEngine.validate(&plan, &self.objects).is_err();
                (too_old || invalid).then_some(plan.id)
            })
            .collect::<Vec<_>>();
        if stale.is_empty() {
            Ok(())
        } else {
            Err(ServiceError::InvalidInput(format!(
                "发现 {} 个陈旧或已失效计划：{}",
                stale.len(),
                stale.join(", ")
            )))
        }
    }

    fn check_backups(&self) -> ServiceResult<()> {
        for backup in self.list_backups()? {
            self.verify_backup(&backup.id)?;
        }
        Ok(())
    }

    fn available_secret_values(&self, revision: &AssetRevision) -> ServiceResult<Vec<SecretValue>> {
        let mut values = Vec::new();
        for reference in secret_refs(revision) {
            match self.vault.get(&reference) {
                Ok(value) => values.push(value),
                // 卸载和诊断不应因用户已经删除 secret 而被阻塞。
                Err(rigdeck_security::SecretError::NotFound(_)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(values)
    }

    fn audit_secret_change(&self, event_type: &str, reference: &SecretRef) -> ServiceResult<()> {
        let created_at_ms = now_ms();
        let event = rigdeck_core::AuditEvent {
            id: ContentHash::from_bytes(
                format!(
                    "audit\0{event_type}\0{}\0{created_at_ms}",
                    reference.as_str()
                )
                .as_bytes(),
            )
            .to_string(),
            event_type: event_type.to_owned(),
            plan_id: None,
            details: serde_json::json!({ "secret_ref": reference.as_str() }),
            created_at_ms,
        };
        self.database.save_audit_event(&event)?;
        Ok(())
    }

    fn persist_import(&self, mut imported: ImportedSkill) -> ServiceResult<AssetInspection> {
        persist_imported_skill(&imported, &self.objects)?;
        let files = file_summaries(&imported.content);
        imported.asset.current_revision_id = Some(imported.revision.id.clone());
        self.database.save_revision(&imported.revision)?;
        self.database.save_asset(&imported.asset, now_ms())?;
        Ok(AssetInspection {
            asset: imported.asset,
            revision: imported.revision,
            files,
        })
    }

    fn persist_single_asset(
        &self,
        path: &Utf8Path,
        declared_name: String,
        kind: AssetKind,
        bytes: Vec<u8>,
        spec: AssetSpec,
    ) -> ServiceResult<AssetInspection> {
        let package = path.file_name().unwrap_or("local-asset").to_owned();
        self.persist_single_asset_from_source(
            "local",
            &package,
            ".",
            Source {
                kind: SourceKind::LocalFile,
                namespace: "local".to_owned(),
                locator: path.to_string(),
                revision: None,
            },
            declared_name,
            kind,
            bytes,
            spec,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_single_asset_from_source(
        &self,
        source_namespace: &str,
        package: &str,
        relative_path: &str,
        source: Source,
        declared_name: String,
        kind: AssetKind,
        bytes: Vec<u8>,
        spec: AssetSpec,
    ) -> ServiceResult<AssetInspection> {
        if spec.kind() != kind {
            return Err(ServiceError::InvalidInput(
                "资产 kind 与 spec 不一致".to_owned(),
            ));
        }
        let identity = AssetIdentity::new(source_namespace, package, relative_path, declared_name)?;
        let mut asset = Asset::new(identity, kind);
        let raw_hash = ContentHash::from_bytes(&bytes);
        let content_object = self.objects.put_bytes(&bytes)?;
        if content_object != raw_hash {
            return Err(ServiceError::InvalidInput(
                "对象库返回的内容 hash 不一致".to_owned(),
            ));
        }
        let revision = AssetRevision {
            id: ContentHash::from_bytes(format!("revision\0{}\0{raw_hash}", asset.id).as_bytes())
                .to_string(),
            raw_hash,
            normalized_hash: normalized_hash(&bytes),
            content_object,
            source,
            license: None,
            audit: AuditResult {
                schema_version: 1,
                completed: true,
                findings: Vec::new(),
            },
            spec,
            created_at_ms: now_ms(),
            author: None,
            update_time_ms: None,
            platform_restrictions: Vec::new(),
        };
        asset.current_revision_id = Some(revision.id.clone());
        self.database.save_revision(&revision)?;
        self.database.save_asset(&asset, now_ms())?;
        let content = AssetContent::single("content", bytes);
        Ok(AssetInspection {
            asset,
            revision,
            files: file_summaries(&content),
        })
    }

    fn persist_fetched_skill(
        &self,
        fetched: FetchedCatalogAsset,
    ) -> ServiceResult<AssetInspection> {
        self.persist_import(imported_from_fetched(fetched)?)
    }

    fn baselines_by_instance(&self) -> ServiceResult<BTreeMap<String, Vec<BaselineFile>>> {
        let mut latest = BTreeMap::<Utf8PathBuf, (ContentHash, bool)>::new();
        for snapshot in self.database.list_snapshots()? {
            for target in snapshot.removed_targets {
                let previous = latest
                    .get(&target)
                    .map(|(hash, _)| hash.clone())
                    .unwrap_or_else(|| ContentHash::from_bytes(&[]));
                latest.insert(target, (previous, true));
            }
            for (target, hash) in snapshot.target_hashes {
                latest.insert(target, (hash, false));
            }
        }
        let mut output = BTreeMap::new();
        for instance in self.database.list_agent_instances()? {
            let baselines = latest
                .iter()
                .filter(|(path, _)| {
                    instance
                        .managed_roots
                        .iter()
                        .any(|root| path.starts_with(root))
                })
                .map(|(path, (hash, expected_absent))| BaselineFile {
                    path: path.clone(),
                    raw_hash: hash.clone(),
                    normalized_hash: hash.clone(),
                    source_hash: None,
                    expected_absent: *expected_absent,
                })
                .collect();
            output.insert(instance.id, baselines);
        }
        Ok(output)
    }

    fn owned_files_for_assignment(&self, assignment_id: &str) -> ServiceResult<Vec<Utf8PathBuf>> {
        let mut files = BTreeSet::new();
        for snapshot in self.database.list_snapshots()? {
            let Some(plan) = self.database.load_plan(&snapshot.plan_id)? else {
                continue;
            };
            if plan.assignment_id.as_deref() == Some(assignment_id)
                && !matches!(plan.purpose, PlanPurpose::Remove)
            {
                files.extend(snapshot.target_hashes.into_keys());
            }
        }
        Ok(files.into_iter().collect())
    }

    fn plan_remove_assignment(
        &self,
        inspection: &AssetInspection,
        assignment: &Assignment,
        source_hashes: Vec<ContentHash>,
    ) -> ServiceResult<DeploymentPlan> {
        let instance = self
            .database
            .load_agent_instance(&assignment.agent_instance_id)?
            .ok_or_else(|| {
                ServiceError::NotFound(format!("Agent 实例 {}", assignment.agent_instance_id))
            })?;
        let adapter = BuiltinAdapter::load(&instance.adapter_id)?;
        let adapter_plan = adapter.plan_remove(&inspection.asset, &instance, &assignment.scope)?;
        if !adapter_plan.manual_required.is_empty() {
            return Err(ServiceError::ManualRequired(
                adapter_plan.manual_required.join("；"),
            ));
        }
        let owned_files = self.owned_files_for_assignment(&assignment.id)?;
        let mut planner = Planner::new(Some(assignment.id.clone()), source_hashes, &self.objects)
            .with_purpose(PlanPurpose::Remove);
        add_removals_to_planner(
            &mut planner,
            &instance,
            &adapter_plan.removals,
            &owned_files,
        )?;
        let known = self.available_secret_values(&inspection.revision)?;
        planner.sanitize_rendered_diffs(|diff| rigdeck_security::redact_text(diff, &known));
        let plan = planner.finish(now_ms())?;
        self.database.save_plan(&plan)?;
        Ok(plan)
    }

    fn load_revision_content(
        &self,
        revision: &rigdeck_core::AssetRevision,
    ) -> ServiceResult<AssetContent> {
        match &revision.spec {
            AssetSpec::Skill(skill) => {
                let inventory = self.objects.get_bytes(&skill.inventory_object)?;
                let value: serde_json::Value = serde_json::from_slice(&inventory)?;
                let files = value
                    .get("files")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| {
                        ServiceError::InvalidInput("Skill inventory 缺少 files".to_owned())
                    })?
                    .iter()
                    .map(|file| {
                        let path = file
                            .get("path")
                            .and_then(serde_json::Value::as_str)
                            .ok_or_else(|| {
                                ServiceError::InvalidInput("Skill inventory path 无效".to_owned())
                            })?;
                        let hash: ContentHash = file
                            .get("raw_hash")
                            .and_then(serde_json::Value::as_str)
                            .ok_or_else(|| {
                                ServiceError::InvalidInput(
                                    "Skill inventory raw_hash 无效".to_owned(),
                                )
                            })?
                            .parse()?;
                        Ok(AssetFileContent {
                            relative_path: path.into(),
                            bytes: self.objects.get_bytes(&hash)?,
                            executable: file
                                .get("executable")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                        })
                    })
                    .collect::<ServiceResult<Vec<_>>>()?;
                Ok(AssetContent { files })
            }
            _ => Ok(AssetContent::single(
                "content",
                self.objects.get_bytes(&revision.content_object)?,
            )),
        }
    }
}

fn imported_from_fetched(fetched: FetchedCatalogAsset) -> ServiceResult<ImportedSkill> {
    let FetchedCatalogAsset::Skill(FetchedAsset {
        source,
        source_namespace,
        package,
        relative_path,
        content,
        license,
        ..
    }) = fetched
    else {
        return Err(ServiceError::InvalidInput(
            "该目录条目只有 MCP metadata，不能按 Skill 导入".to_owned(),
        ));
    };
    Ok(import_skill_bundle(
        content,
        source,
        source_namespace,
        package,
        relative_path,
        license,
        now_ms(),
    )?)
}

fn load_or_create_object_key(vault: &dyn SecretVault) -> ServiceResult<ObjectKey> {
    let reference = SecretRef::new(OBJECT_KEY_REF)?;
    match vault.get(&reference) {
        Ok(value) => {
            let bytes: [u8; 32] = value.expose().try_into().map_err(|_| {
                ServiceError::InvalidInput("钥匙串中的对象库密钥长度不是 32 字节".to_owned())
            })?;
            Ok(ObjectKey::from_bytes(bytes))
        }
        Err(rigdeck_security::SecretError::NotFound(_)) => {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).map_err(|error| {
                ServiceError::InvalidInput(format!("无法生成对象库密钥：{error}"))
            })?;
            vault.put(&reference, &SecretValue::new(bytes.to_vec()))?;
            Ok(ObjectKey::from_bytes(bytes))
        }
        Err(error) => Err(error.into()),
    }
}

fn content_object_hashes(
    content: &AssetContent,
    revision: &rigdeck_core::AssetRevision,
) -> ServiceResult<Vec<ContentHash>> {
    let mut hashes: BTreeSet<_> = content
        .files
        .iter()
        .map(|file| ContentHash::from_bytes(&file.bytes))
        .collect();
    if let AssetSpec::Skill(skill) = &revision.spec {
        hashes.insert(skill.inventory_object.clone());
    }
    Ok(hashes.into_iter().collect())
}

fn desired_bytes_for(
    plan: &DeploymentPlan,
    path: &Utf8Path,
    objects: &ObjectStore,
) -> ServiceResult<Vec<u8>> {
    let operation = plan
        .operations
        .iter()
        .find(|operation| operation.target_path == path)
        .ok_or_else(|| ServiceError::InvalidInput(format!("RigDeck 侧计划没有目标路径：{path}")))?;
    if operation.kind != OperationKind::WriteFile {
        return Err(ServiceError::ManualRequired(format!(
            "RigDeck 侧对 {path} 的期望不是可合并文本"
        )));
    }
    let object = operation
        .desired_object
        .as_ref()
        .ok_or_else(|| ServiceError::InvalidInput(format!("RigDeck 写入操作缺少对象：{path}")))?;
    Ok(objects.get_bytes(object)?)
}

fn add_selected_rigdeck_operation(
    planner: &mut Planner<'_>,
    rigdeck: &DeploymentPlan,
    path: &Utf8Path,
    objects: &ObjectStore,
) -> ServiceResult<()> {
    let operation = rigdeck
        .operations
        .iter()
        .find(|operation| operation.target_path == path)
        .ok_or_else(|| ServiceError::InvalidInput(format!("RigDeck 侧计划没有目标路径：{path}")))?;
    match operation.kind {
        OperationKind::WriteFile => {
            let object = operation.desired_object.as_ref().ok_or_else(|| {
                ServiceError::InvalidInput(format!("RigDeck 写入操作缺少对象：{path}"))
            })?;
            planner.write_file(
                path.to_owned(),
                &objects.get_bytes(object)?,
                "逐文件选择：采用 RigDeck 当前修订",
                operation.compatibility_losses.clone(),
                operation.risk,
            )?;
        }
        OperationKind::RemoveFile => {
            planner.remove_file(
                path.to_owned(),
                "逐文件选择：采用 RigDeck 删除意图",
                operation.risk,
            )?;
        }
        OperationKind::AdoptBaseline => {
            return Err(ServiceError::InvalidInput(
                "RigDeck 侧计划不应包含 adopt_baseline".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_resolution_text(value: &str) -> ServiceResult<Vec<u8>> {
    if value.is_empty() || value.len() > MAX_SINGLE_ASSET_BYTES as usize || value.contains('\0') {
        return Err(ServiceError::InvalidInput(
            "合并正文必须为 1..=1 MiB 的 UTF-8 文本且不能包含 NUL".to_owned(),
        ));
    }
    Ok(value.as_bytes().to_vec())
}

fn automatic_merge(
    path: &Utf8Path,
    base: &[u8],
    ours: &[u8],
    theirs: &[u8],
) -> ServiceResult<Vec<u8>> {
    let base_text = std::str::from_utf8(base)
        .map_err(|_| ServiceError::ManualRequired("三方合并只支持 UTF-8 文本".to_owned()))?;
    let ours_text = std::str::from_utf8(ours)
        .map_err(|_| ServiceError::ManualRequired("三方合并只支持 UTF-8 文本".to_owned()))?;
    let theirs_text = std::str::from_utf8(theirs)
        .map_err(|_| ServiceError::ManualRequired("三方合并只支持 UTF-8 文本".to_owned()))?;
    if path.extension() == Some("json") {
        let base_json: serde_json::Value = serde_json::from_str(base_text)?;
        let ours_json: serde_json::Value = serde_json::from_str(ours_text)?;
        let theirs_json: serde_json::Value = serde_json::from_str(theirs_text)?;
        return match merge_json(&base_json, &ours_json, &theirs_json) {
            MergeResult::Clean { value } => Ok(serde_json::to_vec_pretty(&value)?),
            MergeResult::Conflict { paths, .. } => Err(ServiceError::ManualRequired(format!(
                "JSON 三方合并在 {} 重叠，请提交人工 merged_content",
                paths.join("、")
            ))),
        };
    }
    match merge_text(base_text, ours_text, theirs_text) {
        MergeResult::Clean { value } => Ok(value.into_bytes()),
        MergeResult::Conflict { paths, .. } => Err(ServiceError::ManualRequired(format!(
            "文本三方合并在 {} 重叠，请提交人工 merged_content",
            paths.join("、")
        ))),
    }
}

fn file_summaries(content: &AssetContent) -> Vec<AssetFileSummary> {
    let mut files: Vec<_> = content
        .files
        .iter()
        .map(|file| AssetFileSummary {
            path: file.relative_path.clone(),
            hash: ContentHash::from_bytes(&file.bytes),
            size: u64::try_from(file.bytes.len()).unwrap_or(u64::MAX),
            executable: file.executable,
        })
        .collect();
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

fn secret_refs(revision: &rigdeck_core::AssetRevision) -> Vec<SecretRef> {
    let AssetSpec::McpServer(server) = &revision.spec else {
        return Vec::new();
    };
    let bindings = match &server.transport {
        rigdeck_core::McpTransport::Stdio { env, .. } => env,
        rigdeck_core::McpTransport::StreamableHttp { headers, .. } => headers,
    };
    bindings
        .values()
        .filter_map(|value| match value {
            rigdeck_core::BindingValue::Secret(reference) => Some(reference.clone()),
            rigdeck_core::BindingValue::Literal(_) => None,
        })
        .collect()
}

fn read_safe_single_file(path: &Utf8Path) -> ServiceResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ServiceError::InvalidInput(format!("无法读取本地资产：{error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ServiceError::InvalidInput(
            "本地单文件资产必须是普通文件，不能是目录或 symlink".to_owned(),
        ));
    }
    if metadata.len() > MAX_SINGLE_ASSET_BYTES {
        return Err(ServiceError::InvalidInput(format!(
            "本地单文件资产超过 {MAX_SINGLE_ASSET_BYTES} 字节上限"
        )));
    }
    fs::read(path).map_err(|error| ServiceError::InvalidInput(error.to_string()))
}

fn validate_mcp_spec(spec: &McpServerSpec) -> ServiceResult<()> {
    AssetIdentity::new("validation", "mcp", ".", &spec.server_name)?;
    let bindings = match &spec.transport {
        McpTransport::Stdio { command, env, .. } => {
            if command.trim().is_empty() || command.chars().any(char::is_control) {
                return Err(ServiceError::InvalidInput(
                    "MCP stdio command 不能为空或包含控制字符".to_owned(),
                ));
            }
            env
        }
        McpTransport::StreamableHttp { url, headers } => {
            let parsed = url::Url::parse(url)
                .map_err(|error| ServiceError::InvalidInput(format!("MCP URL 无效：{error}")))?;
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err(ServiceError::InvalidInput(
                    "MCP URL 禁止携带 userinfo 凭据".to_owned(),
                ));
            }
            let loopback = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
            if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
                return Err(ServiceError::InvalidInput(
                    "远端 MCP 必须使用 HTTPS；HTTP 仅允许 loopback".to_owned(),
                ));
            }
            headers
        }
    };
    for (key, value) in bindings {
        if key.trim().is_empty() || key.chars().any(char::is_control) {
            return Err(ServiceError::InvalidInput(
                "MCP 绑定名称不能为空或包含控制字符".to_owned(),
            ));
        }
        let normalized = key.to_ascii_uppercase().replace('-', "_");
        let sensitive = ["TOKEN", "SECRET", "PASSWORD", "AUTHORIZATION", "API_KEY"]
            .iter()
            .any(|needle| normalized.contains(needle));
        if sensitive && matches!(value, BindingValue::Literal(_)) {
            return Err(ServiceError::InvalidInput(format!(
                "敏感 MCP 绑定 {key} 必须使用 SecretRef"
            )));
        }
    }
    Ok(())
}

fn check_result<T, E: std::fmt::Display>(id: &str, result: Result<T, E>) -> DoctorCheck {
    match result {
        Ok(_) => DoctorCheck {
            id: id.to_owned(),
            healthy: true,
            message: format!("{id} 检查通过"),
        },
        Err(error) => DoctorCheck {
            id: id.to_owned(),
            healthy: false,
            message: error.to_string(),
        },
    }
}

fn now_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use rigdeck_security::InMemorySecretVault;

    use super::*;

    fn service() -> (tempfile::TempDir, RigDeckService) {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join("data")).unwrap();
        let vault = Arc::new(InMemorySecretVault::default());
        vault
            .put(
                &SecretRef::new(OBJECT_KEY_REF).unwrap(),
                &SecretValue::new(vec![7; 32]),
            )
            .unwrap();
        let service = RigDeckService::open_with_key(
            AppPaths::for_root(root),
            ObjectKey::from_bytes([7; 32]),
            vault,
        )
        .unwrap();
        (temp, service)
    }

    fn wait_for_watch_refresh(service: &mut RigDeckService) -> WatchRefreshOutcome {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(outcome) = service.poll_watcher(Duration::from_millis(100)).unwrap() {
                return outcome;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "watcher 应在 2 秒预算内触发刷新"
            );
        }
    }

    #[test]
    fn local_skill_import_status_and_inspection_share_one_store() {
        let (temp, service) = service();
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("skill")).unwrap();
        fs::create_dir_all(skill.join("scripts")).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: service-demo\nlicense: MIT\n---\n# Demo\n",
        )
        .unwrap();
        fs::write(skill.join("scripts/run.sh"), b"echo safe\n").unwrap();
        let imported = service.add_local_skill(&skill).unwrap();
        assert_eq!(service.status().unwrap().asset_count, 1);
        assert_eq!(service.inspect(&imported.asset.id).unwrap(), imported);
        assert!(service.doctor().healthy);
    }

    #[test]
    fn refresh_detects_fixture_and_persists_inventory() {
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home")).unwrap();
        let project = Utf8PathBuf::from_path_buf(temp.path().join("project")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(home.join(".codex/AGENTS.md"), b"user").unwrap();
        let outcome = service.refresh(home, Some(project)).unwrap();
        assert!(outcome
            .instances
            .iter()
            .any(|result| result.instance.adapter_id == "codex"));
        assert_eq!(service.status().unwrap().agents.len(), 1);
    }

    #[test]
    fn doctor_detects_invalidated_pending_plan() {
        let (temp, service) = service();
        let target = Utf8PathBuf::from_path_buf(temp.path().join("pending.txt")).unwrap();
        let source_hash = rigdeck_core::ContentStore::put(&service.objects, b"desired").unwrap();
        let mut planner = Planner::new(None, vec![source_hash], &service.objects);
        planner
            .write_file(
                target.clone(),
                b"desired",
                "创建 doctor 测试文件",
                Vec::new(),
                rigdeck_core::RiskLevel::Low,
            )
            .unwrap();
        let plan = planner.finish(now_ms()).unwrap();
        service.database.save_plan(&plan).unwrap();
        assert!(service.doctor().healthy);

        // 计划产生后目标被其他进程创建，expected_target_hash 不再成立。
        fs::write(target, b"external").unwrap();
        let report = service.doctor();
        let check = report
            .checks
            .iter()
            .find(|check| check.id == "stale_plans")
            .unwrap();
        assert!(!report.healthy);
        assert!(!check.healthy);
        assert!(check.message.contains(&plan.id));
    }

    #[test]
    fn prompt_and_mcp_share_plan_apply_remove_lifecycle() {
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        let instructions = home.join(".codex/AGENTS.md");
        fs::write(&instructions, b"user instructions\n").unwrap();
        let instance = service
            .refresh(home.clone(), None)
            .unwrap()
            .instances
            .into_iter()
            .find(|item| item.instance.adapter_id == "codex")
            .unwrap()
            .instance;

        let prompt_path = Utf8PathBuf::from_path_buf(temp.path().join("review.md")).unwrap();
        fs::write(&prompt_path, b"Review changes carefully.\n").unwrap();
        let prompt = service
            .add_local_prompt(&prompt_path, Some("review-rules"), &["global".to_owned()])
            .unwrap();
        assert_eq!(prompt.asset.kind, AssetKind::Prompt);
        let prompt_plan = service
            .plan_assignment(&prompt.asset.id, &instance.id, "global")
            .unwrap();
        service.apply_plan(&prompt_plan.id).unwrap();
        assert!(fs::read_to_string(&instructions)
            .unwrap()
            .contains("Review changes carefully."));
        let prompt_remove = service.plan_remove_asset(&prompt.asset.id).unwrap();
        service.apply_plan(&prompt_remove[0].id).unwrap();
        assert_eq!(fs::read(&instructions).unwrap(), b"user instructions\n");

        let mcp_path = Utf8PathBuf::from_path_buf(temp.path().join("context-mcp.json")).unwrap();
        let mcp_spec = McpServerSpec {
            server_name: "context-mcp".to_owned(),
            transport: McpTransport::Stdio {
                command: "context-server".to_owned(),
                args: vec!["--stdio".to_owned()],
                env: BTreeMap::new(),
            },
            enabled: true,
            timeout_ms: Some(30_000),
            oauth: None,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
        };
        fs::write(&mcp_path, serde_json::to_vec_pretty(&mcp_spec).unwrap()).unwrap();
        let mcp = service.add_local_mcp(&mcp_path).unwrap();
        assert_eq!(mcp.asset.kind, AssetKind::McpServer);
        let mcp_plan = service
            .plan_assignment(&mcp.asset.id, &instance.id, "global")
            .unwrap();
        let mcp_target = mcp_plan.operations[0].target_path.clone();
        service.apply_plan(&mcp_plan.id).unwrap();
        assert!(fs::read_to_string(&mcp_target)
            .unwrap()
            .contains("context-mcp"));
        let mcp_remove = service.plan_remove_asset(&mcp.asset.id).unwrap();
        service.apply_plan(&mcp_remove[0].id).unwrap();
        assert!(!fs::read_to_string(&mcp_target)
            .unwrap_or_default()
            .contains("context-mcp"));
    }

    #[test]
    fn mcp_sensitive_literal_is_rejected_before_inventory_write() {
        let (temp, service) = service();
        let path = Utf8PathBuf::from_path_buf(temp.path().join("unsafe-mcp.json")).unwrap();
        let value = serde_json::json!({
            "server_name": "unsafe-mcp",
            "transport": {
                "type": "streamable_http",
                "url": "https://mcp.example.test",
                "headers": {
                    "Authorization": { "binding": "literal", "value": "Bearer plaintext" }
                }
            },
            "enabled": true,
            "timeout_ms": 30000,
            "oauth": null,
            "allowed_tools": [],
            "denied_tools": []
        });
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        assert!(service.add_local_mcp(&path).is_err());
        assert_eq!(service.status().unwrap().asset_count, 0);
    }

    #[test]
    fn registry_import_accepts_unique_remote_and_rejects_package_only() {
        let (_temp, service) = service();
        let item = rigdeck_registry::CatalogItem {
            id: "io.github.acme/context".to_owned(),
            provider_id: "official-mcp-preview".to_owned(),
            kind: AssetKind::McpServer,
            name: "io.github.acme/context".to_owned(),
            description: Some("Context server".to_owned()),
            version: Some("1.2.3".to_owned()),
            license: None,
            locator: "mcp-registry:official-mcp-preview/io.github.acme/context".to_owned(),
            namespace_verified: true,
            metadata: serde_json::Value::Null,
            vulnerabilities: Vec::new(),
        };
        let fetched = FetchedCatalogAsset::McpMetadata {
            item: item.clone(),
            server_json: serde_json::json!({
                "name": item.name,
                "version": "1.2.3",
                "remotes": [{
                    "type": "streamable-http",
                    "url": "https://mcp.example.test/context"
                }]
            }),
        };
        let imported = service.add_fetched_mcp(fetched, None).unwrap();
        assert_eq!(imported.asset.identity.declared_name, "context");
        assert_eq!(imported.revision.source.kind, SourceKind::McpRegistry);

        let package_only = FetchedCatalogAsset::McpMetadata {
            item,
            server_json: serde_json::json!({
                "name": "io.github.acme/context",
                "packages": [{
                    "registryType": "npm",
                    "identifier": "@acme/context",
                    "version": "1.2.3",
                    "transport": { "type": "stdio" }
                }]
            }),
        };
        assert!(matches!(
            service.add_fetched_mcp(package_only, None),
            Err(ServiceError::ManualRequired(_))
        ));
    }

    #[test]
    fn secret_value_never_enters_database_objects_backup_or_audit_json() {
        let (temp, service) = service();
        let plaintext = b"never-persist-this-secret".to_vec();
        let reference = "keychain:mcp/context-token";
        service.set_secret(reference, plaintext.clone()).unwrap();
        assert!(service.has_secret(reference).unwrap());
        let activity = serde_json::to_vec(&service.activity(10).unwrap()).unwrap();
        assert!(!activity
            .windows(plaintext.len())
            .any(|window| window == plaintext));

        let mcp_path = Utf8PathBuf::from_path_buf(temp.path().join("secret-mcp.json")).unwrap();
        let mcp_spec = McpServerSpec {
            server_name: "secret-mcp".to_owned(),
            transport: McpTransport::StreamableHttp {
                url: "https://mcp.example.test/context".to_owned(),
                headers: BTreeMap::from([(
                    "Authorization".to_owned(),
                    BindingValue::Secret(SecretRef::new(reference).unwrap()),
                )]),
            },
            enabled: true,
            timeout_ms: Some(30_000),
            oauth: None,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
        };
        fs::write(&mcp_path, serde_json::to_vec(&mcp_spec).unwrap()).unwrap();
        service.add_local_mcp(&mcp_path).unwrap();
        let export = Utf8PathBuf::from_path_buf(temp.path().join("portable.json")).unwrap();
        service.export_bundle(&export).unwrap();
        let export_bytes = fs::read(export).unwrap();
        assert!(!export_bytes
            .windows(plaintext.len())
            .any(|window| window == plaintext));
        service.create_backup().unwrap();

        fn assert_tree_clean(root: &Utf8Path, plaintext: &[u8]) {
            for entry in fs::read_dir(root).unwrap() {
                let entry = entry.unwrap();
                let path = Utf8PathBuf::from_path_buf(entry.path()).unwrap();
                if path.is_dir() {
                    assert_tree_clean(&path, plaintext);
                } else {
                    let bytes = fs::read(&path).unwrap();
                    assert!(
                        !bytes
                            .windows(plaintext.len())
                            .any(|window| window == plaintext),
                        "持久化文件泄漏 secret：{path}"
                    );
                }
            }
        }
        assert_tree_clean(&service.paths.data_root, &plaintext);
        service.delete_secret(reference).unwrap();
        service.delete_secret(reference).unwrap();
        assert!(!service.has_secret(reference).unwrap());
    }

    #[test]
    fn install_and_remove_plans_change_assignment_only_after_apply() {
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/AGENTS.md"), b"user").unwrap();
        // `refresh` 接收并拥有 `Utf8PathBuf`；测试稍后还要再次刷新，所以这里克隆一份。
        // Rust 的所有权规则可避免原路径在后台异步流程中被悬空引用。
        let outcome = service.refresh(home.clone(), None).unwrap();
        let instance = outcome
            .instances
            .iter()
            .find(|item| item.instance.adapter_id == "codex")
            .unwrap()
            .instance
            .clone();

        let skill = Utf8PathBuf::from_path_buf(temp.path().join("managed-skill")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: managed-skill\n---\n# Managed\n",
        )
        .unwrap();
        let imported = service.add_local_skill(&skill).unwrap();
        let install = service
            .plan_assignment(&imported.asset.id, &instance.id, "global")
            .unwrap();
        assert_eq!(install.purpose, PlanPurpose::Install);
        assert!(!service.database.list_assignments().unwrap()[0].enabled);
        let target = install.operations[0].target_path.clone();
        service.apply_plan(&install.id).unwrap();
        assert!(target.is_file());
        assert!(service.database.list_assignments().unwrap()[0].enabled);
        assert!(service.apply_plan(&install.id).is_err());

        let assignment_id = service.database.list_assignments().unwrap()[0].id.clone();
        let disable = service
            .plan_assignment_enabled(&assignment_id, false)
            .unwrap();
        assert_eq!(disable.purpose, PlanPurpose::Remove);
        service.apply_plan(&disable.id).unwrap();
        assert!(!target.exists());
        assert!(!service.database.list_assignments().unwrap()[0].enabled);
        let enable = service
            .plan_assignment_enabled(&assignment_id, true)
            .unwrap();
        assert_eq!(enable.purpose, PlanPurpose::Update);
        service.apply_plan(&enable.id).unwrap();
        assert!(target.is_file());
        assert!(service.database.list_assignments().unwrap()[0].enabled);

        let remove = service.plan_remove_asset(&imported.asset.id).unwrap();
        assert_eq!(remove.len(), 1);
        assert_eq!(remove[0].purpose, PlanPurpose::Remove);
        service.apply_plan(&remove[0].id).unwrap();
        assert!(!target.exists());
        assert!(!service.database.list_assignments().unwrap()[0].enabled);

        // 删除快照会留下“该路径应当不存在”的 tombstone。这里模拟用户或其他工具
        // 又创建了同名文件，下一次 refresh 必须把它识别为专门的重建冲突，
        // 而不是把它误判成一个全新的、无人管理的文件。
        let latest = service.database.latest_snapshot().unwrap().unwrap();
        assert!(latest.removed_targets.contains(&target));
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"recreated outside RigDeck\n").unwrap();
        let refreshed = service.refresh(home, None).unwrap();
        let recreated = refreshed
            .instances
            .iter()
            .flat_map(|result| &result.report.items)
            .find(|item| item.path == target)
            .expect("重建后的路径应出现在刷新报告中");
        assert_eq!(recreated.state, rigdeck_core::DriftState::Conflict);
        assert_eq!(
            recreated.conflict.as_ref().map(|value| &value.kind),
            Some(&rigdeck_core::ConflictKind::RecreatedAfterRemoval)
        );
    }

    #[test]
    fn conflict_resolution_plan_applies_before_resolved_and_refreshes_stably() {
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/AGENTS.md"), b"user").unwrap();
        let instance = service
            .refresh(home.clone(), None)
            .unwrap()
            .instances
            .into_iter()
            .find(|item| item.instance.adapter_id == "codex")
            .unwrap()
            .instance;
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("fork-skill")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        let rigdeck_bytes = b"---\nname: fork-skill\n---\n# RigDeck\n";
        fs::write(skill.join("SKILL.md"), rigdeck_bytes).unwrap();
        let imported = service.add_local_skill(&skill).unwrap();
        let install = service
            .plan_assignment(&imported.asset.id, &instance.id, "global")
            .unwrap();
        let target = install.operations[0].target_path.clone();
        service.apply_plan(&install.id).unwrap();

        fs::write(&target, b"---\nname: fork-skill\n---\n# Agent fork\n").unwrap();
        service.refresh(home.clone(), None).unwrap();
        let conflict = service.list_conflicts(Some(false)).unwrap().remove(0);
        assert!(conflict.assignment_id.is_some());
        assert!(conflict.current_hash.is_some());
        let fork_plan = service
            .resolve_conflict(&conflict.id, ResolutionAction::KeepAgentFork)
            .unwrap();
        assert_eq!(fork_plan.purpose, PlanPurpose::ConflictResolution);
        assert_eq!(
            fork_plan.operations[0].kind,
            rigdeck_core::OperationKind::AdoptBaseline
        );
        assert!(!service.conflict(&conflict.id).unwrap().resolved);
        service.apply_plan(&fork_plan.id).unwrap();
        assert!(service.conflict(&conflict.id).unwrap().resolved);
        let stable = service.refresh(home.clone(), None).unwrap();
        let fork_item = stable
            .instances
            .iter()
            .flat_map(|result| &result.report.items)
            .find(|item| item.path == target)
            .unwrap();
        assert_eq!(fork_item.state, rigdeck_core::DriftState::ManagedClean);

        fs::write(&target, b"---\nname: fork-skill\n---\n# Change again\n").unwrap();
        service.refresh(home.clone(), None).unwrap();
        let conflict = service.list_conflicts(Some(false)).unwrap().remove(0);
        let keep_plan = service
            .resolve_conflict(&conflict.id, ResolutionAction::KeepRigdeckRevision)
            .unwrap();
        assert!(!service.conflict(&conflict.id).unwrap().resolved);
        service.apply_plan(&keep_plan.id).unwrap();
        assert_eq!(fs::read(&target).unwrap(), rigdeck_bytes);
        assert!(service.conflict(&conflict.id).unwrap().resolved);
        let stable = service.refresh(home, None).unwrap();
        let keep_item = stable
            .instances
            .iter()
            .flat_map(|result| &result.report.items)
            .find(|item| item.path == target)
            .unwrap();
        assert_eq!(keep_item.state, rigdeck_core::DriftState::ManagedClean);
    }

    #[test]
    fn importing_agent_revision_switches_catalog_only_after_apply() {
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home-import-conflict")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        let instance = service
            .refresh(home.clone(), None)
            .unwrap()
            .instances
            .into_iter()
            .find(|item| item.instance.adapter_id == "codex")
            .unwrap()
            .instance;
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("import-conflict-skill")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: import-conflict-skill\n---\n# RigDeck\n",
        )
        .unwrap();
        let imported = service.add_local_skill(&skill).unwrap();
        let original_revision = imported.revision.id.clone();
        let install = service
            .plan_assignment(&imported.asset.id, &instance.id, "global")
            .unwrap();
        let target = install.operations[0].target_path.clone();
        service.apply_plan(&install.id).unwrap();
        let agent_bytes = b"---\nname: import-conflict-skill\n---\n# Agent revision\n";
        fs::write(&target, agent_bytes).unwrap();
        service.refresh(home.clone(), None).unwrap();
        let conflict = service.list_conflicts(Some(false)).unwrap().remove(0);
        assert!(conflict
            .actions
            .contains(&ResolutionAction::ImportAgentRevision));

        let plan = service
            .resolve_conflict_request(
                &conflict.id,
                ConflictResolutionRequest {
                    action: ResolutionAction::ImportAgentRevision,
                    rename_to: None,
                    backup_id: None,
                    merged_content: None,
                    files: Vec::new(),
                },
            )
            .unwrap();
        assert_eq!(plan.catalog_effects.len(), 2);
        assert_eq!(
            service.inspect(&imported.asset.id).unwrap().revision.id,
            original_revision
        );
        service.apply_plan(&plan.id).unwrap();
        let switched = service.inspect(&imported.asset.id).unwrap();
        assert_ne!(switched.revision.id, original_revision);
        assert_eq!(
            service
                .database
                .list_assignments()
                .unwrap()
                .into_iter()
                .find(|assignment| assignment.asset_id == imported.asset.id)
                .unwrap()
                .revision_id,
            switched.revision.id
        );
        assert_eq!(
            service.refresh(home, None).unwrap().instances[0]
                .report
                .items[0]
                .state,
            rigdeck_core::DriftState::ManagedClean
        );
    }

    #[test]
    fn parameterized_conflict_actions_are_planned_and_superseded_safely() {
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home-actions")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        let instance = service
            .refresh(home.clone(), None)
            .unwrap()
            .instances
            .into_iter()
            .find(|item| item.instance.adapter_id == "codex")
            .unwrap()
            .instance;
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("action-skill")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        let rigdeck_bytes = b"---\nname: action-skill\n---\n# RigDeck\n";
        fs::write(skill.join("SKILL.md"), rigdeck_bytes).unwrap();
        let imported = service.add_local_skill(&skill).unwrap();
        let install = service
            .plan_assignment(&imported.asset.id, &instance.id, "global")
            .unwrap();
        let target = install.operations[0].target_path.clone();
        service.apply_plan(&install.id).unwrap();
        let backup = service.create_backup().unwrap();
        let agent_bytes = b"---\nname: action-skill\n---\n# Agent\n";
        fs::write(&target, agent_bytes).unwrap();
        service.refresh(home, None).unwrap();
        let conflict = service.list_conflicts(Some(false)).unwrap().remove(0);

        let rename = service
            .resolve_conflict_request(
                &conflict.id,
                ConflictResolutionRequest {
                    action: ResolutionAction::RenameAndCoexist,
                    rename_to: Some("action-skill-agent".to_owned()),
                    backup_id: None,
                    merged_content: None,
                    files: Vec::new(),
                },
            )
            .unwrap();
        assert!(rename
            .catalog_effects
            .iter()
            .any(|effect| matches!(effect, CatalogEffect::UpsertAsset { .. })));

        let merge = service
            .resolve_conflict_request(
                &conflict.id,
                ConflictResolutionRequest {
                    action: ResolutionAction::ThreeWayMerge,
                    rename_to: None,
                    backup_id: None,
                    merged_content: None,
                    files: Vec::new(),
                },
            )
            .unwrap();
        assert_eq!(
            service.database.plan_status(&rename.id).unwrap().unwrap(),
            "abandoned"
        );
        assert_eq!(merge.operations[0].kind, OperationKind::AdoptBaseline);

        let selected = service
            .resolve_conflict_request(
                &conflict.id,
                ConflictResolutionRequest {
                    action: ResolutionAction::PerFileSelection,
                    rename_to: None,
                    backup_id: None,
                    merged_content: None,
                    files: vec![FileResolution {
                        path: target.clone(),
                        choice: FileResolutionChoice::Merged,
                        merged_content: Some("人工审查后的正文\n".to_owned()),
                    }],
                },
            )
            .unwrap();
        assert_eq!(selected.operations.len(), 1);
        assert_eq!(
            service.database.plan_status(&merge.id).unwrap().unwrap(),
            "abandoned"
        );

        let restore = service
            .resolve_conflict_request(
                &conflict.id,
                ConflictResolutionRequest {
                    action: ResolutionAction::RestoreBackup,
                    rename_to: None,
                    backup_id: Some(backup.id),
                    merged_content: None,
                    files: Vec::new(),
                },
            )
            .unwrap();
        assert_eq!(
            desired_bytes_for(&restore, &target, &service.objects).unwrap(),
            rigdeck_bytes
        );

        let abandon = service
            .resolve_conflict_request(
                &conflict.id,
                ConflictResolutionRequest {
                    action: ResolutionAction::AbandonPlan,
                    rename_to: None,
                    backup_id: None,
                    merged_content: None,
                    files: Vec::new(),
                },
            )
            .unwrap();
        assert!(abandon.operations.is_empty());
        assert_eq!(
            service.database.plan_status(&restore.id).unwrap().unwrap(),
            "abandoned"
        );
        service.apply_plan(&abandon.id).unwrap();
        let still_open = service.conflict(&conflict.id).unwrap();
        assert!(!still_open.resolved);
        assert!(still_open.resolution_plan_id.is_none());
    }

    #[test]
    fn watcher_reclassifies_add_modify_delete_and_rename_within_budget() {
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home-watcher")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        let instance = service
            .refresh(home.clone(), None)
            .unwrap()
            .instances
            .into_iter()
            .find(|item| item.instance.adapter_id == "codex")
            .unwrap()
            .instance;
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("watch-skill")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        let rigdeck_bytes = b"---\nname: watch-skill\n---\n# RigDeck\n";
        fs::write(skill.join("SKILL.md"), rigdeck_bytes).unwrap();
        let imported = service.add_local_skill(&skill).unwrap();
        let install = service
            .plan_assignment(&imported.asset.id, &instance.id, "global")
            .unwrap();
        let target = install.operations[0].target_path.clone();
        service.apply_plan(&install.id).unwrap();
        // 应用期间会产生 watcher 事件；重新刷新会丢弃旧句柄/队列并从干净状态监听。
        service.refresh(home.clone(), None).unwrap();

        let external = target
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("external-watch")
            .join("SKILL.md");
        fs::create_dir_all(external.parent().unwrap()).unwrap();
        fs::write(&external, b"---\nname: external-watch\n---\n# External\n").unwrap();
        let added = wait_for_watch_refresh(&mut service);
        // 某些平台先上报父目录 create，再上报文件 create；任一事件都会触发可信全量扫描。
        assert!(!added.changed_paths.is_empty());
        assert!(added
            .refresh
            .instances
            .iter()
            .flat_map(|result| &result.report.items)
            .any(
                |item| item.path == external && item.state == rigdeck_core::DriftState::ExternalNew
            ));

        fs::write(&target, b"---\nname: watch-skill\n---\n# Modified\n").unwrap();
        let modified = wait_for_watch_refresh(&mut service);
        assert!(modified
            .refresh
            .instances
            .iter()
            .flat_map(|result| &result.report.items)
            .any(|item| {
                item.path == target && item.state == rigdeck_core::DriftState::ManagedModified
            }));

        let conflict = service
            .list_conflicts(Some(false))
            .unwrap()
            .into_iter()
            .find(|conflict| conflict.affected.iter().any(|path| path == target.as_str()))
            .unwrap();
        let restore = service
            .resolve_conflict(&conflict.id, ResolutionAction::KeepRigdeckRevision)
            .unwrap();
        service.apply_plan(&restore.id).unwrap();
        service.refresh(home.clone(), None).unwrap();

        let renamed_root = target
            .parent()
            .unwrap()
            .with_file_name("watch-skill-renamed");
        fs::rename(target.parent().unwrap(), &renamed_root).unwrap();
        let moved = wait_for_watch_refresh(&mut service);
        assert!(moved
            .refresh
            .instances
            .iter()
            .flat_map(|result| &result.report.items)
            .any(|item| item.change == rigdeck_core::ObservedChange::Renamed));

        fs::remove_dir_all(&renamed_root).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, rigdeck_bytes).unwrap();
        service.refresh(home, None).unwrap();
        fs::remove_file(&target).unwrap();
        let removed = wait_for_watch_refresh(&mut service);
        assert!(removed
            .refresh
            .instances
            .iter()
            .flat_map(|result| &result.report.items)
            .any(|item| item.path == target
                && item.state == rigdeck_core::DriftState::ExternalRemoved));
    }

    #[test]
    fn pin_unpin_archive_restore_lifecycle_round_trips() {
        let (temp, service) = service();
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("lifecycle-skill")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: lifecycle-skill\n---\n# Lifecycle\n",
        )
        .unwrap();
        let inspection = service.add_local_skill(&skill).unwrap();
        let asset_id = &inspection.asset.id;

        // pin
        let pinned = service.pin_asset(asset_id).unwrap();
        assert_eq!(pinned.state, rigdeck_core::AssetState::Pinned);
        // 再次 pin 是幂等的
        let pinned_again = service.pin_asset(asset_id).unwrap();
        assert_eq!(pinned_again.state, rigdeck_core::AssetState::Pinned);

        // unpin
        let unpinned = service.unpin_asset(asset_id).unwrap();
        assert_eq!(unpinned.state, rigdeck_core::AssetState::Active);

        // archive
        let archived = service.archive_asset(asset_id).unwrap();
        assert_eq!(archived.state, rigdeck_core::AssetState::Archived);
        // 再次 archive 是幂等的
        let archived_again = service.archive_asset(asset_id).unwrap();
        assert_eq!(archived_again.state, rigdeck_core::AssetState::Archived);

        // 归档状态下不能 pin
        assert!(service.pin_asset(asset_id).is_err());

        // restore
        let restored = service.restore_asset(asset_id).unwrap();
        assert_eq!(restored.state, rigdeck_core::AssetState::Active);

        // 非 Archived 状态不能 restore
        assert!(service.restore_asset(asset_id).is_err());

        // 审计事件已记录
        let events = service.activity(100).unwrap();
        assert!(events
            .iter()
            .any(|event| event.event_type == "asset_pinned"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "asset_archived"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "asset_restored"));
    }

    #[test]
    fn pin_archive_on_missing_asset_fails() {
        let (_temp, service) = service();
        assert!(service.pin_asset("nonexistent").is_err());
        assert!(service.archive_asset("nonexistent").is_err());
        assert!(service.restore_asset("nonexistent").is_err());
        assert!(service.unpin_asset("nonexistent").is_err());
    }

    #[test]
    fn import_legacy_registry_imports_skills() {
        let (temp, service) = service();
        // 模拟旧 Registry 结构：skills/platform/loki/SKILL.md
        let registry = Utf8PathBuf::from_path_buf(temp.path().join("old-registry")).unwrap();
        let loki = registry.join("skills/platform/loki");
        fs::create_dir_all(&loki).unwrap();
        fs::write(loki.join("SKILL.md"), b"---\nname: loki\n---\n# Loki\n").unwrap();

        let zabbix = registry.join("skills/platform/zabbix");
        fs::create_dir_all(&zabbix).unwrap();
        fs::write(
            zabbix.join("SKILL.md"),
            b"---\nname: zabbix\n---\n# Zabbix\n",
        )
        .unwrap();

        // 一个没有 SKILL.md 的目录应被跳过
        let empty = registry.join("skills/workflow/empty");
        fs::create_dir_all(&empty).unwrap();

        let report = service.import_legacy_registry(&registry).unwrap();
        assert_eq!(report.imported.len(), 2);
        assert!(report.skipped.iter().any(|s| s.contains("empty")));
    }

    #[test]
    fn import_legacy_registry_rejects_invalid_root() {
        let (temp, service) = service();
        let bad = Utf8PathBuf::from_path_buf(temp.path().join("nope")).unwrap();
        assert!(service.import_legacy_registry(&bad).is_err());
    }

    #[tokio::test]
    async fn update_assets_respects_cancel_flag() {
        let (temp, service) = service();
        // 导入两个本地 Skill，确保 update_assets 有工作要做。
        for name in ["skill-a", "skill-b"] {
            let skill = Utf8PathBuf::from_path_buf(temp.path().join(name)).unwrap();
            fs::create_dir_all(&skill).unwrap();
            fs::write(
                skill.join("SKILL.md"),
                format!("---\nname: {name}\n---\n# {name}\n"),
            )
            .unwrap();
            service.add_local_skill(&skill).unwrap();
        }
        // 设置取消标志后调用 update_assets。
        service.request_cancel();
        let report = service.update_assets().await.unwrap();
        assert!(service.is_cancelled());
        // 取消后应跳过所有资产。
        assert_eq!(report.checked, 2);
        assert!(report.updated.is_empty());
        assert!(report.skipped.iter().any(|s| s.contains("用户取消")));
        // 清除标志后可正常更新。
        service.clear_cancel();
        assert!(!service.is_cancelled());
        let report = service.update_assets().await.unwrap();
        assert_eq!(report.checked, 2);
        // 内容未变，updated 仍为空但 skipped 不含取消信息。
        assert!(!report.skipped.iter().any(|s| s.contains("用户取消")));
    }

    #[test]
    fn plan_assignment_is_deterministic_across_invocations() {
        // GUI/CLI 等价性：相同状态和输入必须产生完全相同的 plan。
        // CLI 和 Tauri IPC 都调用同一个 RigDeckService::plan_assignment，
        // 此测试验证该方法在相同条件下是确定性的。
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/AGENTS.md"), b"user").unwrap();
        let outcome = service.refresh(home, None).unwrap();
        let instance = outcome
            .instances
            .iter()
            .find(|item| item.instance.adapter_id == "codex")
            .unwrap()
            .instance
            .clone();

        let skill = Utf8PathBuf::from_path_buf(temp.path().join("equiv-skill")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: equiv-skill\n---\n# Equivalence\n",
        )
        .unwrap();
        let inspection = service.add_local_skill(&skill).unwrap();
        let asset_id = &inspection.asset.id;

        // 两次调用 plan_assignment 应产生等价的 plan。
        // plan ID 包含时间戳因此每次不同；purpose 在首次调用后从 Install 变为 Update
        // 是预期行为。operations 必须确定性等价——这是 GUI/CLI 等价性的核心。
        let plan1 = service
            .plan_assignment(asset_id, &instance.id, "global")
            .unwrap();
        let plan2 = service
            .plan_assignment(asset_id, &instance.id, "global")
            .unwrap();

        assert_eq!(plan1.schema_version, plan2.schema_version);
        assert_eq!(plan1.operations.len(), plan2.operations.len());
        for (op1, op2) in plan1.operations.iter().zip(plan2.operations.iter()) {
            assert_eq!(op1.kind, op2.kind, "operation kind 必须等价");
            assert_eq!(op1.target_path, op2.target_path, "target_path 必须等价");
            assert_eq!(op1.desired_hash, op2.desired_hash, "desired_hash 必须等价");
        }
    }

    #[test]
    fn plan_remove_asset_is_deterministic_across_invocations() {
        // GUI/CLI 等价性：plan_remove_asset 也必须是确定性的。
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/AGENTS.md"), b"user").unwrap();
        let outcome = service.refresh(home, None).unwrap();
        let instance = outcome
            .instances
            .iter()
            .find(|item| item.instance.adapter_id == "codex")
            .unwrap()
            .instance
            .clone();

        let skill = Utf8PathBuf::from_path_buf(temp.path().join("remove-equiv")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: remove-equiv\n---\n# Remove Equivalence\n",
        )
        .unwrap();
        let inspection = service.add_local_skill(&skill).unwrap();
        let asset_id = &inspection.asset.id;

        // 先安装，再测试卸载 plan 的确定性。
        let install = service
            .plan_assignment(asset_id, &instance.id, "global")
            .unwrap();
        service.apply_plan(&install.id).unwrap();

        let plans1 = service.plan_remove_asset(asset_id).unwrap();
        let plans2 = service.plan_remove_asset(asset_id).unwrap();

        assert_eq!(plans1.len(), plans2.len());
        for (p1, p2) in plans1.iter().zip(plans2.iter()) {
            assert_eq!(p1.schema_version, p2.schema_version);
            assert_eq!(p1.operations.len(), p2.operations.len());
            for (op1, op2) in p1.operations.iter().zip(p2.operations.iter()) {
                assert_eq!(op1.kind, op2.kind, "remove operation kind 必须等价");
                assert_eq!(op1.target_path, op2.target_path);
            }
        }
    }

    #[test]
    fn cli_and_desktop_share_core_service_methods() {
        // CLI/Desktop 等价性（Cross-Cutting 项47）：
        // 验证 CLI 和 Tauri IPC 共用的核心 service 方法都存在且可调用。
        // 这是一个编译时 + 运行时双重检查：如果 service 方法签名变更，
        // 此测试会在编译时失败，强制 CLI 和 Tauri 同步更新。
        let (temp, mut service) = service();
        let home = Utf8PathBuf::from_path_buf(temp.path().join("home")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/AGENTS.md"), b"user").unwrap();

        // 验证 CLI 和 Tauri 都调用的核心方法存在
        let _ = service.status();
        let outcome = service.refresh(home, None).unwrap();
        assert!(!outcome.instances.is_empty());
        let _ = service.list_assets().unwrap();
        let _ = service.list_assignments().unwrap();
        let _ = service.list_plans(None).unwrap();
        let _ = service.list_conflicts(Some(false)).unwrap();
        let _ = service.activity(10).unwrap();
        let _ = service.doctor();
        let _ = service.create_backup().unwrap();
        let _ = service.list_backups().unwrap();

        // 验证新增的生命周期方法也存在
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("equiv-lifecycle")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: equiv-lifecycle\n---\n# Lifecycle\n",
        )
        .unwrap();
        let inspection = service.add_local_skill(&skill).unwrap();
        let _ = service.pin_asset(&inspection.asset.id).unwrap();
        let _ = service.unpin_asset(&inspection.asset.id).unwrap();
        let _ = service.archive_asset(&inspection.asset.id).unwrap();
        let _ = service.restore_asset(&inspection.asset.id).unwrap();
    }
}
