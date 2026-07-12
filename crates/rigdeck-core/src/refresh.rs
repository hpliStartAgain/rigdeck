//! 启动刷新、漂移分类与可解释冲突生成。

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::{Conflict, ConflictKind, ContentHash, DriftState, ResolutionAction};

/// 扫描器发现的结构或能力问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationIssue {
    /// 相同逻辑资产与相同内容被重复发现；可自动归并。
    DuplicateIdentical,
    /// 声明名相同，但来源或内容不同。
    NameCollision,
    /// 托管块缺失边界、嵌套或重复。
    DamagedManagedBlock,
    /// 托管 Prompt block 被编辑、移动或重复。
    PromptBlockMoved,
    /// MCP key 与另一个逻辑资产冲突。
    McpKeyCollision,
    /// 目标引用的 SecretRef 不存在或失效。
    MissingSecretBinding,
    /// 目标无法表达必需语义。
    CapabilityLoss,
    /// 目录名与 Skill frontmatter 声明名不同。
    DeclaredNameMismatch,
    /// 大小写、无效名称、symlink、权限或路径边界异常。
    PathAnomaly,
    /// 仅大小写不同的重命名。
    CaseOnlyRename,
    /// 已成功卸载的内容被 Agent 自动重新创建。
    RecreatedAfterRemoval,
    /// Adapter 明确不支持该原生状态。
    Unsupported,
    /// 需要用户到外部界面完成。
    ManualRequired,
}

/// 最近一次成功部署留下的文件基线。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineFile {
    /// 目标绝对路径。
    pub path: Utf8PathBuf,
    /// 部署后原始字节 hash。
    pub raw_hash: ContentHash,
    /// 部署后规范化文本 hash。
    pub normalized_hash: ContentHash,
    /// 当时使用的来源修订 hash。
    pub source_hash: Option<ContentHash>,
    /// 最近一次成功计划期望该目标不存在（卸载 tombstone）。
    #[serde(default)]
    pub expected_absent: bool,
}

/// 本次本地扫描观察到的文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedFile {
    /// 当前绝对路径。
    pub path: Utf8PathBuf,
    /// 当前原始字节 hash。
    pub raw_hash: ContentHash,
    /// 当前规范化文本 hash。
    pub normalized_hash: ContentHash,
    /// 可选稳定逻辑资产 ID，用于同名/重复判断。
    pub logical_id: Option<String>,
    /// 可选结构问题。
    pub issue: Option<ObservationIssue>,
}

/// 一条刷新记录对应的文件系统变化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedChange {
    /// 文件未变化。
    Unchanged,
    /// 新增文件。
    Added,
    /// 内容变化。
    Modified,
    /// 文件删除。
    Removed,
    /// 相同规范化内容移动到新路径。
    Renamed,
}

/// 单个文件的刷新分类结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshItem {
    /// 当前路径；删除时为原路径。
    pub path: Utf8PathBuf,
    /// rename 时的旧路径。
    pub previous_path: Option<Utf8PathBuf>,
    /// 统一漂移状态。
    pub state: DriftState,
    /// 文件系统变化。
    pub change: ObservedChange,
    /// 观察问题。
    pub issue: Option<ObservationIssue>,
    /// 需要人工处理时的可解释冲突。
    pub conflict: Option<Conflict>,
}

/// 一次本地刷新摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshReport {
    /// 每个受影响路径的确定性结果。
    pub items: Vec<RefreshItem>,
    /// 各 DriftState 计数，key 使用稳定 snake_case 名称。
    pub counts: BTreeMap<String, usize>,
    /// 扫描开始 Unix 毫秒时间戳。
    pub started_at_ms: i64,
    /// 扫描结束 Unix 毫秒时间戳。
    pub finished_at_ms: i64,
}

/// 可持久化的单实例库存记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryRecord {
    /// 由实例 ID 与路径确定的稳定记录 ID。
    pub id: String,
    /// Agent 实例 ID。
    pub agent_instance_id: String,
    /// 刷新分类。
    pub item: RefreshItem,
    /// 当前 raw hash；已删除条目为 `None`。
    pub raw_hash: Option<ContentHash>,
    /// 当前 normalized hash；已删除条目为 `None`。
    pub normalized_hash: Option<ContentHash>,
    /// 扫描器提供的逻辑资产 ID。
    pub logical_id: Option<String>,
    /// 最近更新时间。
    pub updated_at_ms: i64,
}

impl InventoryRecord {
    /// 从刷新条目和可选当前观察构造稳定记录。
    pub fn from_refresh(
        agent_instance_id: impl Into<String>,
        item: RefreshItem,
        observed: Option<&ObservedFile>,
        updated_at_ms: i64,
    ) -> Self {
        let agent_instance_id = agent_instance_id.into();
        let id = ContentHash::from_bytes(
            format!("inventory\0{agent_instance_id}\0{}", item.path).as_bytes(),
        )
        .to_string();
        Self {
            id,
            agent_instance_id,
            item,
            raw_hash: observed.map(|value| value.raw_hash.clone()),
            normalized_hash: observed.map(|value| value.normalized_hash.clone()),
            logical_id: observed.and_then(|value| value.logical_id.clone()),
            updated_at_ms,
        }
    }
}

/// 比较部署基线、当前文件和来源更新，生成确定性刷新报告。
pub fn classify_refresh(
    baselines: &[BaselineFile],
    observed: &[ObservedFile],
    source_updates: &BTreeSet<Utf8PathBuf>,
    started_at_ms: i64,
    finished_at_ms: i64,
) -> RefreshReport {
    let mut baseline_by_path: BTreeMap<_, _> = baselines
        .iter()
        .map(|item| (item.path.clone(), item))
        .collect();
    let mut observed_by_path: BTreeMap<_, _> = observed
        .iter()
        .map(|item| (item.path.clone(), item))
        .collect();
    let mut items = Vec::new();

    // rename 只在规范化内容唯一匹配时自动识别；多个候选保持 add/remove，避免猜错。
    let removed_paths: Vec<_> = baseline_by_path
        .iter()
        .filter(|(path, baseline)| {
            !baseline.expected_absent && !observed_by_path.contains_key(*path)
        })
        .map(|(path, _)| path.clone())
        .collect();
    let added_paths: Vec<_> = observed_by_path
        .keys()
        .filter(|path| !baseline_by_path.contains_key(*path))
        .cloned()
        .collect();
    for old_path in removed_paths {
        let baseline = baseline_by_path[&old_path];
        let candidates: Vec<_> = added_paths
            .iter()
            .filter(|new_path| {
                observed_by_path
                    .get(*new_path)
                    .is_some_and(|item| item.normalized_hash == baseline.normalized_hash)
            })
            .cloned()
            .collect();
        if let [new_path] = candidates.as_slice() {
            let current = observed_by_path.remove(new_path).expect("候选来自 map");
            baseline_by_path.remove(&old_path);
            let source_changed =
                source_updates.contains(&old_path) || source_updates.contains(new_path);
            // 仅大小写不同的路径重命名需要显式提示，避免在大小写敏感文件系统上
            // 静默丢失资产身份。
            let case_only_rename =
                old_path.as_str().eq_ignore_ascii_case(new_path.as_str())
                    && old_path.as_str() != new_path.as_str();
            let (state, conflict) = if case_only_rename {
                (
                    DriftState::Conflict,
                    Some(conflict_for(
                        ConflictKind::CaseOnlyRename,
                        "仅大小写不同的重命名",
                        vec![old_path.to_string(), new_path.to_string()],
                    )),
                )
            } else if source_changed {
                (
                    DriftState::Conflict,
                    Some(conflict_for(
                        ConflictKind::ConcurrentModification,
                        "来源更新期间 Agent 侧还移动了托管文件",
                        vec![old_path.to_string(), new_path.to_string()],
                    )),
                )
            } else {
                (DriftState::ManagedModified, None)
            };
            items.push(RefreshItem {
                path: new_path.clone(),
                previous_path: Some(old_path),
                state,
                change: ObservedChange::Renamed,
                issue: current.issue.clone(),
                conflict,
            });
        }
    }

    let paths: BTreeSet<_> = baseline_by_path
        .keys()
        .chain(observed_by_path.keys())
        .cloned()
        .collect();
    for path in paths {
        let baseline = baseline_by_path.get(&path).copied();
        let current = observed_by_path.get(&path).copied();
        let source_changed = source_updates.contains(&path);
        let (state, change, issue, conflict) =
            classify_one(&path, baseline, current, source_changed);
        items.push(RefreshItem {
            path,
            previous_path: None,
            state,
            change,
            issue,
            conflict,
        });
    }
    items.sort_by(|left, right| left.path.cmp(&right.path));
    let mut counts = BTreeMap::new();
    for item in &items {
        *counts.entry(drift_name(item.state).to_owned()).or_insert(0) += 1;
    }
    RefreshReport {
        items,
        counts,
        started_at_ms,
        finished_at_ms,
    }
}

fn classify_one(
    path: &Utf8PathBuf,
    baseline: Option<&BaselineFile>,
    current: Option<&ObservedFile>,
    source_changed: bool,
) -> (
    DriftState,
    ObservedChange,
    Option<ObservationIssue>,
    Option<Conflict>,
) {
    if baseline.is_some_and(|value| value.expected_absent) {
        return match current {
            None => (
                DriftState::ManagedClean,
                ObservedChange::Unchanged,
                None,
                None,
            ),
            Some(_) => (
                DriftState::Conflict,
                ObservedChange::Added,
                Some(ObservationIssue::RecreatedAfterRemoval),
                Some(conflict_for(
                    ConflictKind::RecreatedAfterRemoval,
                    "已卸载内容被 Agent 自动重新创建",
                    vec![path.to_string()],
                )),
            ),
        };
    }
    if let Some(issue) = current.and_then(|item| item.issue.clone()) {
        return classify_issue(path, issue);
    }
    match (baseline, current) {
        (None, Some(_)) => (DriftState::ExternalNew, ObservedChange::Added, None, None),
        (Some(_), None) => (
            DriftState::ExternalRemoved,
            ObservedChange::Removed,
            None,
            None,
        ),
        (Some(baseline), Some(current)) if baseline.raw_hash == current.raw_hash => {
            if source_changed {
                (
                    DriftState::SourceUpdateAvailable,
                    ObservedChange::Unchanged,
                    None,
                    None,
                )
            } else {
                (
                    DriftState::ManagedClean,
                    ObservedChange::Unchanged,
                    None,
                    None,
                )
            }
        }
        (Some(_), Some(_)) if source_changed => (
            DriftState::Conflict,
            ObservedChange::Modified,
            None,
            Some(conflict_for(
                ConflictKind::ConcurrentModification,
                "来源和 Agent 侧都从同一部署基线发生了变化",
                vec![path.to_string()],
            )),
        ),
        (Some(_), Some(_)) => (
            DriftState::ManagedModified,
            ObservedChange::Modified,
            None,
            Some(conflict_for(
                ConflictKind::ManagedModified,
                "Agent 或用户修改了 RigDeck 已部署内容",
                vec![path.to_string()],
            )),
        ),
        (None, None) => unreachable!("路径来自 baseline/current 并集"),
    }
}

fn classify_issue(
    path: &Utf8PathBuf,
    issue: ObservationIssue,
) -> (
    DriftState,
    ObservedChange,
    Option<ObservationIssue>,
    Option<Conflict>,
) {
    let identical_duplicate = matches!(issue, ObservationIssue::DuplicateIdentical);
    let result = match issue {
        ObservationIssue::Unsupported => (DriftState::Unsupported, None),
        ObservationIssue::ManualRequired => (DriftState::ManualRequired, None),
        ObservationIssue::DuplicateIdentical => (
            DriftState::ManagedClean,
            Some((ConflictKind::DuplicateIdentical, "相同逻辑资产与内容重复")),
        ),
        ObservationIssue::NameCollision => (
            DriftState::Conflict,
            Some((ConflictKind::NameCollision, "同名资产的来源或内容不同")),
        ),
        ObservationIssue::DamagedManagedBlock => (
            DriftState::Conflict,
            Some((ConflictKind::DamagedManagedBlock, "托管块结构损坏")),
        ),
        ObservationIssue::PromptBlockMoved => (
            DriftState::Conflict,
            Some((
                ConflictKind::PromptBlockMoved,
                "托管 Prompt block 被编辑、移动或重复",
            )),
        ),
        ObservationIssue::McpKeyCollision => (
            DriftState::Conflict,
            Some((ConflictKind::McpKeyCollision, "MCP server key 冲突")),
        ),
        ObservationIssue::MissingSecretBinding => (
            DriftState::Conflict,
            Some((
                ConflictKind::MissingSecretBinding,
                "MCP SecretRef 缺失或无效",
            )),
        ),
        ObservationIssue::CapabilityLoss => (
            DriftState::Conflict,
            Some((ConflictKind::CapabilityLoss, "目标存在阻断型能力损失")),
        ),
        ObservationIssue::DeclaredNameMismatch => (
            DriftState::Conflict,
            Some((
                ConflictKind::DeclaredNameMismatch,
                "目录名与资产声明名不一致",
            )),
        ),
        ObservationIssue::PathAnomaly => (
            DriftState::Conflict,
            Some((
                ConflictKind::PathAnomaly,
                "路径、大小写、symlink 或权限异常",
            )),
        ),
        ObservationIssue::CaseOnlyRename => (
            DriftState::Conflict,
            Some((
                ConflictKind::CaseOnlyRename,
                "仅大小写不同的重命名",
            )),
        ),
        ObservationIssue::RecreatedAfterRemoval => (
            DriftState::Conflict,
            Some((
                ConflictKind::RecreatedAfterRemoval,
                "已卸载内容被 Agent 自动重新创建",
            )),
        ),
    };
    let mut conflict = result
        .1
        .map(|(kind, cause)| conflict_for(kind, cause, vec![path.to_string()]));
    if identical_duplicate {
        if let Some(conflict) = &mut conflict {
            conflict.risk = "内容相同，可安全归并为一个逻辑资产".to_owned();
            conflict.actions = vec![ResolutionAction::KeepRigdeckRevision];
            conflict.resolved = true;
        }
    }
    (result.0, ObservedChange::Modified, Some(issue), conflict)
}

fn conflict_for(kind: ConflictKind, cause: &str, affected: Vec<String>) -> Conflict {
    let material = format!("{kind:?}\0{}\0{}", cause, affected.join("\0"));
    Conflict {
        id: ContentHash::from_bytes(material.as_bytes()).to_string(),
        kind,
        cause: cause.to_owned(),
        affected,
        risk: "自动覆盖可能丢失 Agent 侧或来源侧修改".to_owned(),
        actions: vec![
            ResolutionAction::KeepRigdeckRevision,
            ResolutionAction::ImportAgentRevision,
            ResolutionAction::KeepAgentFork,
            ResolutionAction::RenameAndCoexist,
            ResolutionAction::ThreeWayMerge,
            ResolutionAction::PerFileSelection,
            ResolutionAction::AbandonPlan,
            ResolutionAction::RestoreBackup,
        ],
        resolved: false,
        agent_instance_id: None,
        assignment_id: None,
        baseline_hash: None,
        current_hash: None,
        selected_action: None,
        resolution_plan_id: None,
        resolved_at_ms: None,
    }
}

fn drift_name(state: DriftState) -> &'static str {
    match state {
        DriftState::ManagedClean => "managed_clean",
        DriftState::ManagedModified => "managed_modified",
        DriftState::ExternalNew => "external_new",
        DriftState::ExternalRemoved => "external_removed",
        DriftState::SourceUpdateAvailable => "source_update_available",
        DriftState::Conflict => "conflict",
        DriftState::Unsupported => "unsupported",
        DriftState::ManualRequired => "manual_required",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(value: &[u8]) -> ContentHash {
        ContentHash::from_bytes(value)
    }

    #[test]
    fn classifies_clean_modified_removed_new_and_source_update() {
        let baselines = vec![
            BaselineFile {
                path: "/tmp/clean".into(),
                raw_hash: hash(b"same"),
                normalized_hash: hash(b"same"),
                source_hash: None,
                expected_absent: false,
            },
            BaselineFile {
                path: "/tmp/modified".into(),
                raw_hash: hash(b"old"),
                normalized_hash: hash(b"old"),
                source_hash: None,
                expected_absent: false,
            },
            BaselineFile {
                path: "/tmp/removed".into(),
                raw_hash: hash(b"removed"),
                normalized_hash: hash(b"removed"),
                source_hash: None,
                expected_absent: false,
            },
        ];
        let observed = vec![
            ObservedFile {
                path: "/tmp/clean".into(),
                raw_hash: hash(b"same"),
                normalized_hash: hash(b"same"),
                logical_id: None,
                issue: None,
            },
            ObservedFile {
                path: "/tmp/modified".into(),
                raw_hash: hash(b"new"),
                normalized_hash: hash(b"new"),
                logical_id: None,
                issue: None,
            },
            ObservedFile {
                path: "/tmp/new".into(),
                raw_hash: hash(b"external"),
                normalized_hash: hash(b"external"),
                logical_id: None,
                issue: None,
            },
        ];
        let updates = BTreeSet::from([Utf8PathBuf::from("/tmp/clean")]);
        let report = classify_refresh(&baselines, &observed, &updates, 1, 2);
        let states: BTreeMap<_, _> = report
            .items
            .iter()
            .map(|item| (item.path.as_str(), item.state))
            .collect();
        assert_eq!(states["/tmp/clean"], DriftState::SourceUpdateAvailable);
        assert_eq!(states["/tmp/modified"], DriftState::ManagedModified);
        assert_eq!(states["/tmp/removed"], DriftState::ExternalRemoved);
        assert_eq!(states["/tmp/new"], DriftState::ExternalNew);
    }

    #[test]
    fn concurrent_change_and_damaged_block_generate_explainable_conflicts() {
        let baseline = BaselineFile {
            path: "/tmp/value".into(),
            raw_hash: hash(b"old"),
            normalized_hash: hash(b"old"),
            source_hash: None,
            expected_absent: false,
        };
        let current = ObservedFile {
            path: baseline.path.clone(),
            raw_hash: hash(b"new"),
            normalized_hash: hash(b"new"),
            logical_id: None,
            issue: None,
        };
        let report = classify_refresh(
            std::slice::from_ref(&baseline),
            &[current],
            &BTreeSet::from([baseline.path.clone()]),
            0,
            1,
        );
        let conflict = report.items[0].conflict.as_ref().unwrap();
        assert_eq!(conflict.kind, ConflictKind::ConcurrentModification);
        assert!(!conflict.actions.is_empty());

        let damaged = ObservedFile {
            path: "/tmp/AGENTS.md".into(),
            raw_hash: hash(b"broken"),
            normalized_hash: hash(b"broken"),
            logical_id: None,
            issue: Some(ObservationIssue::DamagedManagedBlock),
        };
        let report = classify_refresh(&[], &[damaged], &BTreeSet::new(), 0, 1);
        assert_eq!(
            report.items[0].conflict.as_ref().unwrap().kind,
            ConflictKind::DamagedManagedBlock
        );
    }

    #[test]
    fn unique_same_content_move_is_detected_as_rename() {
        let baseline = BaselineFile {
            path: "/tmp/old".into(),
            raw_hash: hash(b"value"),
            normalized_hash: hash(b"value"),
            source_hash: None,
            expected_absent: false,
        };
        let current = ObservedFile {
            path: "/tmp/new".into(),
            raw_hash: hash(b"value"),
            normalized_hash: hash(b"value"),
            logical_id: None,
            issue: None,
        };
        let report = classify_refresh(&[baseline], &[current], &BTreeSet::new(), 0, 1);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].change, ObservedChange::Renamed);
        assert_eq!(
            report.items[0].previous_path.as_deref(),
            Some(camino::Utf8Path::new("/tmp/old"))
        );
    }

    #[test]
    fn every_observation_issue_has_expected_state_and_conflict_kind() {
        let cases = [
            (
                ObservationIssue::DuplicateIdentical,
                DriftState::ManagedClean,
                Some(ConflictKind::DuplicateIdentical),
            ),
            (
                ObservationIssue::NameCollision,
                DriftState::Conflict,
                Some(ConflictKind::NameCollision),
            ),
            (
                ObservationIssue::DamagedManagedBlock,
                DriftState::Conflict,
                Some(ConflictKind::DamagedManagedBlock),
            ),
            (
                ObservationIssue::PromptBlockMoved,
                DriftState::Conflict,
                Some(ConflictKind::PromptBlockMoved),
            ),
            (
                ObservationIssue::McpKeyCollision,
                DriftState::Conflict,
                Some(ConflictKind::McpKeyCollision),
            ),
            (
                ObservationIssue::MissingSecretBinding,
                DriftState::Conflict,
                Some(ConflictKind::MissingSecretBinding),
            ),
            (
                ObservationIssue::CapabilityLoss,
                DriftState::Conflict,
                Some(ConflictKind::CapabilityLoss),
            ),
            (
                ObservationIssue::DeclaredNameMismatch,
                DriftState::Conflict,
                Some(ConflictKind::DeclaredNameMismatch),
            ),
            (
                ObservationIssue::PathAnomaly,
                DriftState::Conflict,
                Some(ConflictKind::PathAnomaly),
            ),
            (
                ObservationIssue::CaseOnlyRename,
                DriftState::Conflict,
                Some(ConflictKind::CaseOnlyRename),
            ),
            (
                ObservationIssue::RecreatedAfterRemoval,
                DriftState::Conflict,
                Some(ConflictKind::RecreatedAfterRemoval),
            ),
            (ObservationIssue::Unsupported, DriftState::Unsupported, None),
            (
                ObservationIssue::ManualRequired,
                DriftState::ManualRequired,
                None,
            ),
        ];
        for (issue, state, kind) in cases {
            let observed = ObservedFile {
                path: "/tmp/issue".into(),
                raw_hash: hash(b"value"),
                normalized_hash: hash(b"value"),
                logical_id: None,
                issue: Some(issue.clone()),
            };
            let report = classify_refresh(&[], &[observed], &BTreeSet::new(), 0, 1);
            assert_eq!(report.items[0].state, state, "{issue:?}");
            assert_eq!(
                report.items[0]
                    .conflict
                    .as_ref()
                    .map(|value| value.kind.clone()),
                kind,
                "{issue:?}"
            );
            if let Some(conflict) = &report.items[0].conflict {
                assert!(!conflict.cause.is_empty());
                assert!(!conflict.affected.is_empty());
                assert!(!conflict.risk.is_empty());
                assert!(!conflict.actions.is_empty());
            }
        }
    }

    #[test]
    fn case_only_rename_is_flagged_as_conflict() {
        let baseline = BaselineFile {
            path: "/tmp/Skill.md".into(),
            raw_hash: hash(b"content"),
            normalized_hash: hash(b"content"),
            source_hash: None,
            expected_absent: false,
        };
        let observed = ObservedFile {
            path: "/tmp/skill.md".into(),
            raw_hash: hash(b"content"),
            normalized_hash: hash(b"content"),
            logical_id: None,
            issue: None,
        };
        let report = classify_refresh(
            &[baseline],
            &[observed],
            &BTreeSet::new(),
            0,
            1,
        );
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].state, DriftState::Conflict);
        assert_eq!(
            report.items[0].conflict.as_ref().unwrap().kind,
            ConflictKind::CaseOnlyRename
        );
        assert_eq!(report.items[0].change, ObservedChange::Renamed);
    }

    #[test]
    fn normal_rename_without_case_difference_stays_managed() {
        let baseline = BaselineFile {
            path: "/tmp/old-name.md".into(),
            raw_hash: hash(b"content"),
            normalized_hash: hash(b"content"),
            source_hash: None,
            expected_absent: false,
        };
        let observed = ObservedFile {
            path: "/tmp/new-name.md".into(),
            raw_hash: hash(b"content"),
            normalized_hash: hash(b"content"),
            logical_id: None,
            issue: None,
        };
        let report = classify_refresh(
            &[baseline],
            &[observed],
            &BTreeSet::new(),
            0,
            1,
        );
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].state, DriftState::ManagedModified);
        assert!(report.items[0].conflict.is_none());
    }
}
