//! 原子文件应用、验证与故障回滚。

use std::{fs, io::Write};

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{ContentHash, CoreError, CoreResult, DeploymentPlan, OperationKind, PlannedOperation};

/// Core 对内容寻址对象库的最小依赖。
///
/// `trait` 类似一份行为契约：Core 不关心对象存储来自文件夹、内存还是测试替身，
/// 只要求实现者提供 put/get/contains。这样 `rigdeck-store` 可以依赖 Core，而 Core
/// 不必反向依赖 SQLite crate。
pub trait ContentStore {
    /// 写入不可变对象并返回内容 hash；相同内容应去重。
    fn put(&self, bytes: &[u8]) -> CoreResult<ContentHash>;
    /// 按 hash 读取并校验对象。
    fn get(&self, hash: &ContentHash) -> CoreResult<Vec<u8>>;
    /// 判断对象是否存在。
    fn contains(&self, hash: &ContentHash) -> CoreResult<bool>;
}

/// 故障注入检查点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyCheckpoint {
    /// 修改某个目标前。
    BeforeMutation,
    /// 修改某个目标后、验证前。
    AfterMutation,
    /// 所有文件验证通过、提交数据库前。
    BeforeCommit,
}

/// 测试或运行时取消逻辑使用的故障注入契约。
pub trait FailureInjector {
    /// 在指定检查点返回错误即可触发完整回滚。
    fn check(&self, checkpoint: ApplyCheckpoint, operation_index: usize) -> CoreResult<()>;
}

/// 生产默认：不注入故障。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoFailure;

impl FailureInjector for NoFailure {
    fn check(&self, _checkpoint: ApplyCheckpoint, _operation_index: usize) -> CoreResult<()> {
        Ok(())
    }
}

/// 已应用的单个文件结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedFile {
    /// 操作 ID。
    pub operation_id: String,
    /// 目标路径。
    pub target_path: Utf8PathBuf,
    /// 应用后的 hash；删除为 `None`。
    pub resulting_hash: Option<ContentHash>,
    /// 用于恢复的对象 hash；新建文件为 `None`。
    pub rollback_object: Option<ContentHash>,
}

/// 文件全部验证通过后交给持久化层的提交报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyReport {
    /// 计划 ID。
    pub plan_id: String,
    /// 已应用文件。
    pub files: Vec<AppliedFile>,
}

/// 事务执行器。
#[derive(Debug, Default)]
pub struct TransactionEngine;

impl TransactionEngine {
    /// 只读验证计划结构、对象可用性和目标文件前置条件。
    ///
    /// 该方法不会写入任何文件，供 `doctor` 判断尚未应用的计划是否已经因外部
    /// 修改而失效。把它放在 Core 中，可以确保 CLI、桌面端与真正应用计划时使用
    /// 完全相同的校验语义。
    pub fn validate(&self, plan: &DeploymentPlan, objects: &dyn ContentStore) -> CoreResult<()> {
        plan.validate()?;
        validate_objects(plan, objects)?;
        validate_preconditions(plan)
    }

    /// 应用计划，并把数据库/审计提交作为闭包纳入同一恢复边界。
    ///
    /// `commit` 使用 `FnOnce`：它最多只能被调用一次。这个类型约束能防止调用方在
    /// 重试闭包时重复写数据库。闭包返回错误时，已经修改的文件会按相反顺序恢复。
    pub fn apply<F>(
        &self,
        plan: &DeploymentPlan,
        objects: &dyn ContentStore,
        injector: &dyn FailureInjector,
        commit: F,
    ) -> CoreResult<ApplyReport>
    where
        F: FnOnce(&ApplyReport) -> CoreResult<()>,
    {
        self.validate(plan, objects)?;

        let mut applied: Vec<&PlannedOperation> = Vec::new();
        let result = (|| {
            for (index, operation) in plan.operations.iter().enumerate() {
                injector.check(ApplyCheckpoint::BeforeMutation, index)?;
                apply_operation(operation, objects)?;
                applied.push(operation);
                injector.check(ApplyCheckpoint::AfterMutation, index)?;
                verify_operation(operation)?;
            }

            injector.check(ApplyCheckpoint::BeforeCommit, plan.operations.len())?;
            let report = ApplyReport {
                plan_id: plan.id.clone(),
                files: plan
                    .operations
                    .iter()
                    .map(|operation| AppliedFile {
                        operation_id: operation.id.clone(),
                        target_path: operation.target_path.clone(),
                        resulting_hash: operation.desired_hash.clone(),
                        rollback_object: operation.rollback_object.clone(),
                    })
                    .collect(),
            };
            commit(&report).map_err(|error| CoreError::CommitFailed(error.to_string()))?;
            Ok(report)
        })();

        match result {
            Ok(report) => Ok(report),
            Err(original) => {
                if let Err(rollback) = rollback_operations(&applied, objects) {
                    return Err(CoreError::RollbackFailed(format!(
                        "原始错误：{original}；回滚错误：{rollback}"
                    )));
                }
                Err(original)
            }
        }
    }
}

fn validate_objects(plan: &DeploymentPlan, objects: &dyn ContentStore) -> CoreResult<()> {
    for hash in &plan.source_hashes {
        if !objects.contains(hash)? {
            return Err(CoreError::ObjectUnavailable(format!(
                "计划输入对象不存在：{hash}"
            )));
        }
        let bytes = objects.get(hash)?;
        if ContentHash::from_bytes(&bytes) != *hash {
            return Err(CoreError::ObjectUnavailable(format!(
                "计划输入对象 hash 校验失败：{hash}"
            )));
        }
    }
    for operation in &plan.operations {
        for hash in [
            operation.desired_object.as_ref(),
            operation.rollback_object.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if !objects.contains(hash)? {
                return Err(CoreError::ObjectUnavailable(hash.to_string()));
            }
            // get() 必须重算 hash；这里只判断存在还不足以防止对象库被篡改。
            let bytes = objects.get(hash)?;
            if ContentHash::from_bytes(&bytes) != *hash {
                return Err(CoreError::ObjectUnavailable(format!(
                    "对象 hash 校验失败：{hash}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_preconditions(plan: &DeploymentPlan) -> CoreResult<()> {
    for operation in &plan.operations {
        let current = current_hash(&operation.target_path)?;
        if current != operation.expected_target_hash {
            return Err(CoreError::PlanInvalidated {
                path: operation.target_path.clone().into_std_path_buf(),
            });
        }
    }
    Ok(())
}

fn apply_operation(operation: &PlannedOperation, objects: &dyn ContentStore) -> CoreResult<()> {
    match operation.kind {
        OperationKind::WriteFile => {
            let object = operation.desired_object.as_ref().ok_or_else(|| {
                CoreError::InvalidPlan("write_file 缺少 desired object".to_owned())
            })?;
            let bytes = objects.get(object)?;
            atomic_write(&operation.target_path, &bytes)
        }
        OperationKind::RemoveFile => match fs::remove_file(&operation.target_path) {
            Ok(()) => Ok(()),
            Err(error) => Err(CoreError::io(&operation.target_path, error)),
        },
        OperationKind::AdoptBaseline => Ok(()),
    }
}

fn verify_operation(operation: &PlannedOperation) -> CoreResult<()> {
    let actual = current_hash(&operation.target_path)?;
    if actual != operation.desired_hash {
        return Err(CoreError::VerificationFailed(format!(
            "目标 hash 不符合计划：{}",
            operation.target_path
        )));
    }
    Ok(())
}

fn rollback_operations(
    applied: &[&PlannedOperation],
    objects: &dyn ContentStore,
) -> CoreResult<()> {
    for operation in applied.iter().rev() {
        if operation.kind == OperationKind::AdoptBaseline {
            continue;
        }
        if let Some(backup) = operation.rollback_object.as_ref() {
            let bytes = objects.get(backup)?;
            atomic_write(&operation.target_path, &bytes)?;
        } else if operation.target_path.exists() {
            fs::remove_file(&operation.target_path)
                .map_err(|error| CoreError::io(&operation.target_path, error))?;
        }
    }
    Ok(())
}

fn current_hash(path: &Utf8Path) -> CoreResult<Option<ContentHash>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(ContentHash::from_bytes(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CoreError::io(path, error)),
    }
}

fn atomic_write(target: &Utf8Path, bytes: &[u8]) -> CoreResult<()> {
    let parent = target
        .parent()
        .ok_or_else(|| CoreError::InvalidPlan(format!("目标没有父目录：{target}")))?;
    fs::create_dir_all(parent).map_err(|error| CoreError::io(parent, error))?;

    let mut temporary =
        NamedTempFile::new_in(parent).map_err(|error| CoreError::io(parent, error))?;
    temporary
        .write_all(bytes)
        .map_err(|error| CoreError::io(temporary.path(), error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| CoreError::io(temporary.path(), error))?;
    temporary
        .persist(target)
        .map_err(|error| CoreError::io(target, error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeMap};

    use super::*;
    use crate::{Planner, RiskLevel};

    #[derive(Default)]
    struct MemoryObjects(RefCell<BTreeMap<ContentHash, Vec<u8>>>);

    impl ContentStore for MemoryObjects {
        fn put(&self, bytes: &[u8]) -> CoreResult<ContentHash> {
            let hash = ContentHash::from_bytes(bytes);
            self.0.borrow_mut().insert(hash.clone(), bytes.to_vec());
            Ok(hash)
        }

        fn get(&self, hash: &ContentHash) -> CoreResult<Vec<u8>> {
            self.0
                .borrow()
                .get(hash)
                .cloned()
                .ok_or_else(|| CoreError::ObjectUnavailable(hash.to_string()))
        }

        fn contains(&self, hash: &ContentHash) -> CoreResult<bool> {
            Ok(self.0.borrow().contains_key(hash))
        }
    }

    struct FailAt(usize);

    impl FailureInjector for FailAt {
        fn check(&self, checkpoint: ApplyCheckpoint, operation_index: usize) -> CoreResult<()> {
            let offset = match checkpoint {
                ApplyCheckpoint::BeforeMutation => 0,
                ApplyCheckpoint::AfterMutation => 1,
                ApplyCheckpoint::BeforeCommit => 0,
            };
            let step = operation_index * 2 + offset;
            if step == self.0 {
                return Err(CoreError::CommitFailed(format!("注入故障 {step}")));
            }
            Ok(())
        }
    }

    #[test]
    fn every_injected_file_step_restores_exact_state() {
        let directory = tempfile::tempdir().unwrap();
        let first = Utf8PathBuf::from_path_buf(directory.path().join("a.txt")).unwrap();
        let second = Utf8PathBuf::from_path_buf(directory.path().join("b.txt")).unwrap();
        fs::write(&first, b"old-a").unwrap();
        fs::write(&second, b"old-b").unwrap();
        let objects = MemoryObjects::default();

        for failure_step in 0..4 {
            fs::write(&first, b"old-a").unwrap();
            fs::write(&second, b"old-b").unwrap();
            let mut planner = Planner::new(None, Vec::new(), &objects);
            planner
                .write_file(
                    first.clone(),
                    b"new-a",
                    "a diff",
                    Vec::new(),
                    RiskLevel::Medium,
                )
                .unwrap();
            planner
                .write_file(
                    second.clone(),
                    b"new-b",
                    "b diff",
                    Vec::new(),
                    RiskLevel::Medium,
                )
                .unwrap();
            let plan = planner.finish(0).unwrap();

            let result =
                TransactionEngine.apply(&plan, &objects, &FailAt(failure_step), |_| Ok(()));
            assert!(result.is_err());
            assert_eq!(fs::read(&first).unwrap(), b"old-a");
            assert_eq!(fs::read(&second).unwrap(), b"old-b");
        }
    }

    #[test]
    fn replan_after_success_is_empty() {
        let directory = tempfile::tempdir().unwrap();
        let target = Utf8PathBuf::from_path_buf(directory.path().join("value.txt")).unwrap();
        let objects = MemoryObjects::default();
        let mut planner = Planner::new(None, Vec::new(), &objects);
        planner
            .write_file(target.clone(), b"value", "diff", Vec::new(), RiskLevel::Low)
            .unwrap();
        let plan = planner.finish(0).unwrap();
        TransactionEngine
            .apply(&plan, &objects, &NoFailure, |_| Ok(()))
            .unwrap();

        let mut next = Planner::new(None, Vec::new(), &objects);
        next.write_file(target, b"value", "diff", Vec::new(), RiskLevel::Low)
            .unwrap();
        assert!(next.finish(1).unwrap().operations.is_empty());
    }

    #[test]
    fn adopt_baseline_never_mutates_or_deletes_file_on_commit_failure() {
        let directory = tempfile::tempdir().unwrap();
        let target = Utf8PathBuf::from_path_buf(directory.path().join("agent-fork.txt")).unwrap();
        fs::write(&target, b"agent-owned").unwrap();
        let objects = MemoryObjects::default();
        let mut planner = Planner::new(None, Vec::new(), &objects)
            .with_purpose(crate::PlanPurpose::ConflictResolution);
        planner
            .adopt_baseline(target.clone(), "采纳 Agent 分叉")
            .unwrap();
        let plan = planner.finish(0).unwrap();
        let result = TransactionEngine.apply(&plan, &objects, &NoFailure, |_| {
            Err(CoreError::CommitFailed("模拟数据库提交失败".to_owned()))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"agent-owned");

        let mut planner = Planner::new(None, Vec::new(), &objects)
            .with_purpose(crate::PlanPurpose::ConflictResolution);
        planner
            .adopt_baseline(target.clone(), "采纳 Agent 分叉")
            .unwrap();
        let invalidated = planner.finish(1).unwrap();
        fs::write(&target, b"changed-after-plan").unwrap();
        assert!(matches!(
            TransactionEngine.validate(&invalidated, &objects),
            Err(CoreError::PlanInvalidated { .. })
        ));
    }
}
