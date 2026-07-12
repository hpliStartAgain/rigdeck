//! RigDeck Desktop 的 Tauri 外壳。
//!
//! 本文件只做 IPC 参数转换、并发锁与稳定错误映射。所有 SQLite、对象库、Adapter、
//! Planner 和文件事务都由 `rigdeck-service` 处理，React 前端不会直接修改本地状态。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use rigdeck_adapters::StartupRefreshOutcome;
use rigdeck_core::{ConflictResolutionRequest, DeploymentPlan};
use rigdeck_service::{
    home, AppPaths, AssetInspection, BackupManifest, BundleImportReport, CatalogService,
    DoctorReport, PortableBundle, RigDeckService, ServiceError, ServiceStatus, UpdateReport,
    WatchRefreshOutcome,
};
use serde::Serialize;
use tauri::Manager;
use tokio::sync::Mutex;

/// Tauri 管理的共享状态。
///
/// Rust 提示：`Arc` 是线程安全引用计数指针，允许多个 IPC Future 持有同一服务；
/// `Mutex` 保证数据库/Planner/watcher 串行访问；远端目录使用独立 `CatalogService`，
/// 因此慢网络不会占用这把本地状态锁。
#[derive(Clone)]
struct AppState {
    service: Arc<Mutex<RigDeckService>>,
    catalog: CatalogService,
    startup_refresh: Arc<Mutex<StartupRefreshStatus>>,
}

#[derive(Debug, Clone, Serialize)]
struct StartupRefreshStatus {
    completed: bool,
    outcome: Option<StartupRefreshOutcome>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct IpcError {
    code: &'static str,
    message: String,
}

impl From<ServiceError> for IpcError {
    fn from(error: ServiceError) -> Self {
        let code = match &error {
            ServiceError::InvalidInput(_) => "invalid_input",
            ServiceError::NotFound(_) => "not_found",
            ServiceError::ManualRequired(_) => "manual_required",
            ServiceError::Core(_) => "core_error",
            ServiceError::Store(_) => "store_error",
            ServiceError::Adapter(_) => "adapter_error",
            ServiceError::Registry(_) => "registry_error",
            ServiceError::Secret(_) => "secret_store_error",
            ServiceError::Json(_) => "json_error",
            ServiceError::Cancelled => "cancelled",
        };
        Self {
            code,
            message: rigdeck_security::redact_text(&error.to_string(), &[]),
        }
    }
}

/// 初始化共享服务并运行桌面应用。
pub fn run() {
    rigdeck_security::install_redacted_panic_hook();
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .setup(|app| {
            let service = RigDeckService::open(AppPaths::discover()?)?;
            let catalog = service.catalog();
            let state = AppState {
                service: Arc::new(Mutex::new(service)),
                catalog,
                startup_refresh: Arc::new(Mutex::new(StartupRefreshStatus {
                    completed: false,
                    outcome: None,
                    error: None,
                })),
            };
            app.manage(state.clone());

            // 首屏先渲染，再在 Tauri async runtime 中执行启动扫描。远端目录刷新不在
            // 这条路径上，因此网络不可用不会拖慢本地 Overview。
            tauri::async_runtime::spawn(async move {
                let home = match home() {
                    Ok(home) => home,
                    Err(error) => {
                        let safe = rigdeck_security::redact_text(&error.to_string(), &[]);
                        tracing::warn!(error = %safe, "无法发现 HOME，跳过启动刷新");
                        *state.startup_refresh.lock().await = StartupRefreshStatus {
                            completed: true,
                            outcome: None,
                            error: Some(safe),
                        };
                        return;
                    }
                };
                let project = std::env::current_dir()
                    .ok()
                    .and_then(|path| Utf8PathBuf::from_path_buf(path).ok());
                match state.service.lock().await.refresh(home, project) {
                    Ok(outcome) => {
                        *state.startup_refresh.lock().await = StartupRefreshStatus {
                            completed: true,
                            outcome: Some(outcome),
                            error: None,
                        };
                    }
                    Err(error) => {
                        let safe = rigdeck_security::redact_text(&error.to_string(), &[]);
                        tracing::warn!(error = %safe, "启动刷新失败");
                        *state.startup_refresh.lock().await = StartupRefreshStatus {
                            completed: true,
                            outcome: None,
                            error: Some(safe),
                        };
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            status,
            startup_refresh_status,
            refresh,
            poll_watcher,
            list_assets,
            list_assets_page,
            list_assignments,
            inspect_asset,
            search_skills,
            search_mcp,
            add_skill,
            add_local_asset,
            add_mcp_registry,
            set_secret,
            has_secret,
            delete_secret,
            list_plans,
            plan_assignment,
            plan_assignment_enabled,
            apply_plan,
            update_assets,
            plan_remove_asset,
            list_conflicts,
            show_conflict,
            resolve_conflict,
            list_activity,
            doctor,
            create_backup,
            list_backups,
            restore_backup,
            export_bundle,
            import_bundle,
            import_legacy_registry,
        ])
        .run(tauri::generate_context!())
        .expect("RigDeck 桌面应用启动失败");
}

#[tauri::command]
async fn status(state: tauri::State<'_, AppState>) -> Result<ServiceStatus, IpcError> {
    Ok(state.service.lock().await.status()?)
}

#[tauri::command]
async fn startup_refresh_status(
    state: tauri::State<'_, AppState>,
) -> Result<StartupRefreshStatus, IpcError> {
    Ok(state.startup_refresh.lock().await.clone())
}

#[tauri::command]
async fn refresh(
    state: tauri::State<'_, AppState>,
    project_root: Option<String>,
) -> Result<rigdeck_adapters::StartupRefreshOutcome, IpcError> {
    let project_root = project_root.map(Utf8PathBuf::from);
    Ok(state.service.lock().await.refresh(home()?, project_root)?)
}

#[tauri::command]
async fn poll_watcher(
    state: tauri::State<'_, AppState>,
) -> Result<Option<WatchRefreshOutcome>, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .poll_watcher(std::time::Duration::from_millis(20))?)
}

#[tauri::command]
async fn list_assets(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<rigdeck_core::Asset>, IpcError> {
    Ok(state.service.lock().await.list_assets()?)
}

#[tauri::command]
async fn list_assets_page(
    state: tauri::State<'_, AppState>,
    kind: Option<String>,
    query: Option<String>,
    source_namespace: Option<String>,
    offset: usize,
    limit: usize,
) -> Result<rigdeck_service::AssetPage, IpcError> {
    let kind = match kind.as_deref() {
        None => None,
        Some("skill") => Some(rigdeck_core::AssetKind::Skill),
        Some("prompt") => Some(rigdeck_core::AssetKind::Prompt),
        Some("mcp_server") => Some(rigdeck_core::AssetKind::McpServer),
        Some(other) => {
            return Err(IpcError {
                code: "invalid_input",
                message: format!("未知资产类型筛选：{other}"),
            })
        }
    };
    Ok(state.service.lock().await.list_assets_page(
        kind,
        query.as_deref(),
        source_namespace.as_deref(),
        offset,
        limit,
    )?)
}

#[tauri::command]
async fn list_assignments(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<rigdeck_core::Assignment>, IpcError> {
    Ok(state.service.lock().await.list_assignments()?)
}

#[tauri::command]
async fn inspect_asset(
    state: tauri::State<'_, AppState>,
    asset_id: String,
) -> Result<AssetInspection, IpcError> {
    Ok(state.service.lock().await.inspect(&asset_id)?)
}

#[tauri::command]
async fn search_skills(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: usize,
) -> Result<rigdeck_registry::SearchResult, IpcError> {
    Ok(state.catalog.search_skills(&query, limit).await?)
}

#[tauri::command]
async fn search_mcp(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: usize,
) -> Result<rigdeck_registry::SearchResult, IpcError> {
    Ok(state.catalog.search_mcp(&query, limit).await?)
}

#[tauri::command]
fn add_skill(
    state: tauri::State<'_, AppState>,
    source: String,
) -> Result<AssetInspection, IpcError> {
    // 同步 command 在独立线程执行，可安全 block_on；
    // 避免 async command 中 MutexGuard 跨 await 导致的 Send 要求。
    let service = state.service.clone();
    tauri::async_runtime::block_on(
        async move { service.lock().await.add_skill_source(&source).await },
    )
    .map_err(IpcError::from)
}

#[tauri::command]
async fn add_local_asset(
    state: tauri::State<'_, AppState>,
    source: String,
    kind: String,
    name: Option<String>,
    scopes: Vec<String>,
) -> Result<AssetInspection, IpcError> {
    let path = Utf8PathBuf::from(source);
    let service = state.service.lock().await;
    match kind.as_str() {
        "prompt" => Ok(service.add_local_prompt(&path, name.as_deref(), &scopes)?),
        "mcp_server" => Ok(service.add_local_mcp(&path)?),
        _ => Err(IpcError {
            code: "invalid_input",
            message: "本地单文件资产 kind 只能是 prompt 或 mcp_server".to_owned(),
        }),
    }
}

#[tauri::command]
async fn add_mcp_registry(
    state: tauri::State<'_, AppState>,
    id: String,
    alias: Option<String>,
) -> Result<AssetInspection, IpcError> {
    // 网络请求使用独立 CatalogService，不占用本地状态锁；只有通过安全映射后的
    // metadata 在最终落库阶段才短暂锁定 RigDeckService。
    let fetched = state.catalog.fetch_mcp(&id).await?;
    Ok(state
        .service
        .lock()
        .await
        .add_fetched_mcp(fetched, alias.as_deref())?)
}

#[tauri::command]
async fn set_secret(
    state: tauri::State<'_, AppState>,
    reference: String,
    value: Vec<u8>,
) -> Result<String, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .set_secret(&reference, value)?
        .as_str()
        .to_owned())
}

#[tauri::command]
async fn has_secret(
    state: tauri::State<'_, AppState>,
    reference: String,
) -> Result<bool, IpcError> {
    Ok(state.service.lock().await.has_secret(&reference)?)
}

#[tauri::command]
async fn delete_secret(
    state: tauri::State<'_, AppState>,
    reference: String,
) -> Result<String, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .delete_secret(&reference)?
        .as_str()
        .to_owned())
}

#[tauri::command]
async fn list_plans(
    state: tauri::State<'_, AppState>,
    status: Option<String>,
) -> Result<Vec<DeploymentPlan>, IpcError> {
    Ok(state.service.lock().await.list_plans(status.as_deref())?)
}

#[tauri::command]
async fn plan_assignment(
    state: tauri::State<'_, AppState>,
    asset_id: String,
    agent_instance_id: String,
    scope: String,
) -> Result<DeploymentPlan, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .plan_assignment(&asset_id, &agent_instance_id, &scope)?)
}

#[tauri::command]
async fn plan_assignment_enabled(
    state: tauri::State<'_, AppState>,
    assignment_id: String,
    enabled: bool,
) -> Result<DeploymentPlan, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .plan_assignment_enabled(&assignment_id, enabled)?)
}

#[tauri::command]
async fn apply_plan(
    state: tauri::State<'_, AppState>,
    plan_id: String,
) -> Result<rigdeck_core::ApplyReport, IpcError> {
    Ok(state.service.lock().await.apply_plan(&plan_id)?)
}

#[tauri::command]
fn update_assets(state: tauri::State<'_, AppState>) -> Result<UpdateReport, IpcError> {
    // 同步 command 避免 MutexGuard 跨 await 的 Send 要求。
    let service = state.service.clone();
    tauri::async_runtime::block_on(async move { service.lock().await.update_assets().await })
        .map_err(IpcError::from)
}

#[tauri::command]
async fn plan_remove_asset(
    state: tauri::State<'_, AppState>,
    asset_id: String,
) -> Result<Vec<DeploymentPlan>, IpcError> {
    Ok(state.service.lock().await.plan_remove_asset(&asset_id)?)
}

#[tauri::command]
async fn list_conflicts(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<rigdeck_core::Conflict>, IpcError> {
    Ok(state.service.lock().await.list_conflicts(Some(false))?)
}

#[tauri::command]
async fn show_conflict(
    state: tauri::State<'_, AppState>,
    conflict_id: String,
) -> Result<rigdeck_core::Conflict, IpcError> {
    Ok(state.service.lock().await.conflict(&conflict_id)?)
}

#[tauri::command]
async fn resolve_conflict(
    state: tauri::State<'_, AppState>,
    conflict_id: String,
    request: ConflictResolutionRequest,
) -> Result<DeploymentPlan, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .resolve_conflict_request(&conflict_id, request)?)
}

#[tauri::command]
async fn list_activity(
    state: tauri::State<'_, AppState>,
    limit: usize,
) -> Result<Vec<rigdeck_core::AuditEvent>, IpcError> {
    Ok(state.service.lock().await.activity(limit)?)
}

#[tauri::command]
async fn doctor(state: tauri::State<'_, AppState>) -> Result<DoctorReport, IpcError> {
    Ok(state.service.lock().await.doctor())
}

#[tauri::command]
async fn create_backup(state: tauri::State<'_, AppState>) -> Result<BackupManifest, IpcError> {
    Ok(state.service.lock().await.create_backup()?)
}

#[tauri::command]
async fn list_backups(state: tauri::State<'_, AppState>) -> Result<Vec<BackupManifest>, IpcError> {
    Ok(state.service.lock().await.list_backups()?)
}

#[tauri::command]
async fn restore_backup(
    state: tauri::State<'_, AppState>,
    backup_id: String,
) -> Result<BackupManifest, IpcError> {
    Ok(state.service.lock().await.restore_backup(&backup_id)?)
}

#[tauri::command]
async fn export_bundle(
    state: tauri::State<'_, AppState>,
    output: String,
) -> Result<PortableBundle, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .export_bundle(Utf8Path::new(&output))?)
}

#[tauri::command]
async fn import_bundle(
    state: tauri::State<'_, AppState>,
    input: String,
) -> Result<BundleImportReport, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .import_bundle(Utf8Path::new(&input))?)
}

#[tauri::command]
async fn import_legacy_registry(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<rigdeck_service::LegacyImportReport, IpcError> {
    Ok(state
        .service
        .lock()
        .await
        .import_legacy_registry(Utf8Path::new(&path))?)
}
