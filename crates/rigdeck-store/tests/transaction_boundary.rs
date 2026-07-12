//! 文件系统与 SQLite 共同恢复边界的集成测试。

use std::fs;

use camino::Utf8PathBuf;
use rigdeck_core::{CatalogEffect, CoreError, NoFailure, Planner, RiskLevel, TransactionEngine};
use rigdeck_store::{Database, ObjectKey, ObjectStore};

#[test]
fn successful_apply_commits_snapshot_audit_and_plan_status() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    let target = root.join("agent").join("config.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"old").unwrap();

    let objects = ObjectStore::open(root.join("store"), ObjectKey::from_bytes([1; 32])).unwrap();
    let mut planner = Planner::new(None, Vec::new(), &objects);
    planner
        .write_file(
            target.clone(),
            b"new",
            "- old\n+ new",
            Vec::new(),
            RiskLevel::Medium,
        )
        .unwrap();
    let plan = planner.finish(1).unwrap();
    let mut database = Database::in_memory().unwrap();
    database.save_plan(&plan).unwrap();

    TransactionEngine
        .apply(&plan, &objects, &NoFailure, |report| {
            database
                .commit_apply(&plan, report, 2)
                .map(|_| ())
                .map_err(|error| CoreError::CommitFailed(error.to_string()))
        })
        .unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert_eq!(
        database.plan_status(&plan.id).unwrap().as_deref(),
        Some("applied")
    );
}

#[test]
fn sqlite_commit_failure_restores_files_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    let target = root.join("agent").join("config.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"exact-old-bytes\r\n").unwrap();

    let objects = ObjectStore::open(root.join("store"), ObjectKey::from_bytes([2; 32])).unwrap();
    let mut planner = Planner::new(None, Vec::new(), &objects);
    planner
        .write_file(
            target.clone(),
            b"new",
            "redacted diff",
            Vec::new(),
            RiskLevel::Medium,
        )
        .unwrap();
    let plan = planner.finish(1).unwrap();
    let mut database = Database::in_memory().unwrap();

    // 故意不 save_plan：commit_apply 写 snapshot 时会触发 plan_id 外键错误。
    let result = TransactionEngine.apply(&plan, &objects, &NoFailure, |report| {
        database
            .commit_apply(&plan, report, 2)
            .map(|_| ())
            .map_err(|error| CoreError::CommitFailed(error.to_string()))
    });

    assert!(result.is_err());
    assert_eq!(fs::read(&target).unwrap(), b"exact-old-bytes\r\n");
    assert_eq!(database.plan_status(&plan.id).unwrap(), None);
}

#[test]
fn catalog_effect_failure_rolls_back_database_and_files_together() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    let target = root.join("agent").join("catalog-boundary.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, b"before").unwrap();

    let objects = ObjectStore::open(root.join("store"), ObjectKey::from_bytes([3; 32])).unwrap();
    let mut planner = Planner::new(None, Vec::new(), &objects);
    planner
        .write_file(
            target.clone(),
            b"after",
            "目录事务边界",
            Vec::new(),
            RiskLevel::Medium,
        )
        .unwrap();
    planner.add_catalog_effect(CatalogEffect::SetAssetRevision {
        asset_id: "missing-asset".to_owned(),
        revision_id: "missing-revision".to_owned(),
    });
    let plan = planner.finish(1).unwrap();
    let mut database = Database::in_memory().unwrap();
    database.save_plan(&plan).unwrap();

    let result = TransactionEngine.apply(&plan, &objects, &NoFailure, |report| {
        database
            .commit_apply(&plan, report, 2)
            .map(|_| ())
            .map_err(|error| CoreError::CommitFailed(error.to_string()))
    });

    assert!(result.is_err());
    assert_eq!(fs::read(&target).unwrap(), b"before");
    assert_eq!(
        database.plan_status(&plan.id).unwrap().as_deref(),
        Some("pending")
    );
    assert!(database.latest_snapshot().unwrap().is_none());
}
