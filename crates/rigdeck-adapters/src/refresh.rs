//! 七个 Adapter 的启动检测、扫描与 watcher 启动编排。

use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use camino::Utf8PathBuf;
use rigdeck_adapter_sdk::{
    AdapterError, AdapterErrorCode, AdapterResult, AgentAdapter, DetectionContext,
};
use rigdeck_core::{classify_refresh, AgentInstance, BaselineFile, ObservedFile, RefreshReport};
use serde::{Deserialize, Serialize};

use crate::{BuiltinAdapter, WatchService};

/// 某个 Adapter 在本次启动检测中的状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterDetectionStatus {
    /// Adapter ID。
    pub adapter_id: String,
    /// 检测到的实例数。
    pub instance_count: usize,
    /// 非致命检测/扫描错误；其他 Adapter 仍会继续刷新。
    pub error: Option<String>,
}

/// 一个 Agent 实例的扫描结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceRefresh {
    /// 检测到的实例。
    pub instance: AgentInstance,
    /// 原生文件观察结果。
    pub observed: Vec<ObservedFile>,
    /// 与部署基线比较后的报告。
    pub report: RefreshReport,
}

/// 启动刷新完整结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupRefreshOutcome {
    /// Adapter 级状态，包括未安装或失败的 Agent。
    pub adapters: Vec<AdapterDetectionStatus>,
    /// 成功扫描的实例。
    pub instances: Vec<InstanceRefresh>,
    /// 初始扫描结束后可以开始监听的目录。
    pub watch_roots: Vec<Utf8PathBuf>,
}

impl StartupRefreshOutcome {
    /// 初始扫描完成后启动 watcher。
    pub fn start_watcher(&self, debounce: Duration) -> AdapterResult<WatchService> {
        WatchService::start(&self.watch_roots, debounce)
    }
}

/// 共享启动刷新协调器。
pub struct RefreshCoordinator {
    adapters: Vec<Box<dyn AgentAdapter>>,
}

impl std::fmt::Debug for RefreshCoordinator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RefreshCoordinator")
            .field("adapter_count", &self.adapters.len())
            .finish_non_exhaustive()
    }
}

impl RefreshCoordinator {
    /// 使用任意运行时 Adapter 列表创建协调器。
    pub fn new(adapters: Vec<Box<dyn AgentAdapter>>) -> AdapterResult<Self> {
        if adapters.is_empty() {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "刷新协调器至少需要一个 Adapter",
            ));
        }
        let mut ids = BTreeSet::new();
        for adapter in &adapters {
            let id = &adapter.describe().adapter_id;
            if !ids.insert(id.clone()) {
                return Err(AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    format!("刷新协调器中 Adapter ID 重复：{id}"),
                ));
            }
        }
        Ok(Self { adapters })
    }

    /// 加载全部七个内置 Adapter。
    pub fn with_builtins() -> AdapterResult<Self> {
        let adapters: Vec<Box<dyn AgentAdapter>> = BuiltinAdapter::load_all()?
            .into_iter()
            .map(|adapter| Box::new(adapter) as Box<dyn AgentAdapter>)
            .collect();
        Self::new(adapters)
    }

    /// 每次应用启动都执行检测与本地扫描；单个 Adapter 失败不会阻断其他 Agent。
    pub fn run(
        &self,
        context: &DetectionContext,
        baselines: &BTreeMap<String, Vec<BaselineFile>>,
        source_updates: &BTreeMap<String, BTreeSet<Utf8PathBuf>>,
    ) -> StartupRefreshOutcome {
        let started_at_ms = now_ms();
        let mut statuses = Vec::new();
        let mut results = Vec::new();
        let mut watch_roots = BTreeSet::new();

        for adapter in &self.adapters {
            let adapter_id = adapter.describe().adapter_id.clone();
            let detected = match adapter.detect(context) {
                Ok(instances) => instances,
                Err(error) => {
                    statuses.push(AdapterDetectionStatus {
                        adapter_id,
                        instance_count: 0,
                        error: Some(error.to_string()),
                    });
                    continue;
                }
            };
            let instance_count = detected.len();
            let mut adapter_error = None;
            for instance in detected {
                match adapter.scan(&instance) {
                    Ok(scanned) => {
                        let observed: Vec<_> = scanned
                            .into_iter()
                            .map(|entry| ObservedFile {
                                path: entry.path,
                                raw_hash: entry.raw_hash,
                                normalized_hash: entry.normalized_hash,
                                logical_id: entry.logical_id,
                                issue: entry.issue,
                            })
                            .collect();
                        let report = classify_refresh(
                            baselines.get(&instance.id).map_or(&[], Vec::as_slice),
                            &observed,
                            source_updates.get(&instance.id).unwrap_or(&BTreeSet::new()),
                            started_at_ms,
                            now_ms(),
                        );
                        for root in &instance.managed_roots {
                            if root.is_dir() {
                                watch_roots.insert(root.clone());
                            }
                        }
                        results.push(InstanceRefresh {
                            instance,
                            observed,
                            report,
                        });
                    }
                    Err(error) => {
                        adapter_error = Some(error.to_string());
                    }
                }
            }
            statuses.push(AdapterDetectionStatus {
                adapter_id,
                instance_count,
                error: adapter_error,
            });
        }
        statuses.sort_by(|left, right| left.adapter_id.cmp(&right.adapter_id));
        results.sort_by(|left, right| left.instance.id.cmp(&right.instance.id));
        StartupRefreshOutcome {
            adapters: statuses,
            instances: results,
            watch_roots: watch_roots.into_iter().collect(),
        }
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
    use std::fs;

    use rigdeck_core::DriftState;

    use super::*;

    #[test]
    fn startup_refresh_detects_all_adapters_and_scans_local_state() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let home = root.join("home");
        let project = root.join("project");
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(home.join(".codex/AGENTS.md"), b"user instructions").unwrap();
        let coordinator = RefreshCoordinator::with_builtins().unwrap();
        let outcome = coordinator.run(
            &DetectionContext {
                home: home.clone(),
                project_root: Some(project),
            },
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert_eq!(outcome.adapters.len(), 7);
        let codex = outcome
            .instances
            .iter()
            .find(|result| result.instance.adapter_id == "codex")
            .expect("应检测到 Codex");
        assert!(codex
            .report
            .items
            .iter()
            .any(|item| item.state == DriftState::ExternalNew));
        assert!(outcome.watch_roots.contains(&home));
    }

    #[test]
    fn duplicate_adapter_ids_fail_fast() {
        let first = BuiltinAdapter::load("codex").unwrap();
        let second = BuiltinAdapter::load("codex").unwrap();
        let error = RefreshCoordinator::new(vec![Box::new(first), Box::new(second)]).unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::InvalidManifest);
    }
}
