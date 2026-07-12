//! SQLite metadata、计划、快照与审计。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
};

use camino::{Utf8Path, Utf8PathBuf};
use rigdeck_core::{
    AgentInstance, ApplyReport, Asset, AssetRevision, AssetSpec, Assignment, AuditEvent,
    BindingValue, CatalogEffect, Conflict, ContentHash, DeploymentPlan, DeploymentSnapshot,
    DriftState, InventoryRecord, McpTransport, ObservedFile, RefreshReport, ResolutionAction,
    SurfaceMode,
};
use rusqlite::{params, Connection, DatabaseName, OpenFlags, OptionalExtension};

use crate::{embedded, StoreError, StoreResult, STORE_SCHEMA_VERSION};

/// SQLite metadata store。
pub struct Database {
    connection: Connection,
    path: Option<Utf8PathBuf>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Database")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Database {
    /// 打开数据库。已有文件会在 migration 前复制保护，失败时恢复旧文件。
    pub fn open(path: impl Into<Utf8PathBuf>) -> StoreResult<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| StoreError::io(parent, error))?;
        }
        let protection = path.with_extension("sqlite3.pre-migration");
        let existed = path.exists();
        let mut connection = Connection::open(&path)?;
        if existed {
            if protection.exists() {
                fs::remove_file(&protection).map_err(|error| StoreError::io(&protection, error))?;
            }
            // SQLite backup API 会把 WAL 中已提交页一起复制；裸 fs::copy 可能遗漏
            // 尚未 checkpoint 的数据，因此不能作为 migration 前保护手段。
            connection.backup(DatabaseName::Main, protection.as_std_path(), None)?;
        }
        if let Err(error) = configure_and_migrate(&mut connection) {
            drop(connection);
            if existed {
                remove_sqlite_sidecars(&path)?;
                fs::copy(&protection, &path).map_err(|copy| StoreError::io(&path, copy))?;
            }
            return Err(error);
        }
        if protection.exists() {
            fs::remove_file(&protection).map_err(|error| StoreError::io(&protection, error))?;
        }
        Ok(Self {
            connection,
            path: Some(path),
        })
    }

    /// 创建内存数据库，主要用于测试和只读预览。
    pub fn in_memory() -> StoreResult<Self> {
        let mut connection = Connection::open_in_memory()?;
        configure_and_migrate(&mut connection)?;
        Ok(Self {
            connection,
            path: None,
        })
    }

    /// 以 SQLite 只读模式打开已存在数据库，不运行 migration，也不创建保护副本。
    /// 备份预览使用该入口，保证“生成恢复计划”不会修改备份证据本身。
    pub fn open_read_only(path: impl Into<Utf8PathBuf>) -> StoreResult<Self> {
        let path = path.into();
        if !path.is_file() {
            return Err(StoreError::Integrity(format!("只读数据库不存在：{path}")));
        }
        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let database = Self {
            connection,
            path: Some(path),
        };
        database.integrity_check()?;
        Ok(database)
    }

    /// 数据库文件路径；内存数据库返回 `None`。
    pub fn path(&self) -> Option<&Utf8Path> {
        self.path.as_deref()
    }

    /// 保存资产当前 metadata。
    pub fn save_asset(&self, asset: &Asset, updated_at_ms: i64) -> StoreResult<()> {
        let json = serde_json::to_string(asset)?;
        self.connection.execute(
            "INSERT INTO assets(id, kind, json, updated_at_ms) VALUES(?1, ?2, ?3, ?4)\
             ON CONFLICT(id) DO UPDATE SET kind=excluded.kind, json=excluded.json, updated_at_ms=excluded.updated_at_ms",
            params![asset.id, kind_name(asset.kind), json, updated_at_ms],
        )?;
        Ok(())
    }

    /// 按 ID 读取资产。
    pub fn load_asset(&self, id: &str) -> StoreResult<Option<Asset>> {
        load_json(&self.connection, "SELECT json FROM assets WHERE id=?1", id)
    }

    /// 列出全部资产。
    pub fn list_assets(&self) -> StoreResult<Vec<Asset>> {
        list_json(&self.connection, "SELECT json FROM assets ORDER BY id", [])
    }

    /// 返回资产总数，不反序列化资产 JSON。
    pub fn count_assets(&self) -> StoreResult<usize> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    /// 分页检索资产；筛选在 SQLite 内完成，避免 10,000 条库存先全部反序列化到 UI。
    ///
    /// 返回 `(当前页, 总数)`。`kind` 使用数据库稳定字符串，`query` 同时匹配声明名和
    /// package，`source_namespace` 精确匹配来源。调用方必须先限制分页参数。
    pub fn list_assets_page(
        &self,
        kind: Option<&str>,
        query: &str,
        source_namespace: &str,
        offset: usize,
        limit: usize,
    ) -> StoreResult<(Vec<Asset>, usize)> {
        let query = query
            .trim()
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let source_namespace = source_namespace.trim();
        let where_clause = "WHERE (?1 IS NULL OR kind = ?1)\
             AND (?2 = '' OR lower(json_extract(json, '$.identity.declared_name')) LIKE '%' || ?2 || '%' ESCAPE '\\'\
                  OR lower(json_extract(json, '$.identity.package')) LIKE '%' || ?2 || '%' ESCAPE '\\')\
             AND (?3 = '' OR json_extract(json, '$.identity.source_namespace') = ?3)";
        let total: i64 = self.connection.query_row(
            &format!("SELECT COUNT(*) FROM assets {where_clause}"),
            params![kind, query, source_namespace],
            |row| row.get(0),
        )?;
        let mut statement = self.connection.prepare(&format!(
            "SELECT json FROM assets {where_clause} ORDER BY updated_at_ms DESC, id LIMIT ?4 OFFSET ?5"
        ))?;
        let rows = statement.query_map(
            params![
                kind,
                query,
                source_namespace,
                i64::try_from(limit).unwrap_or(i64::MAX),
                i64::try_from(offset).unwrap_or(i64::MAX),
            ],
            |row| row.get::<_, String>(0),
        )?;
        let mut assets = Vec::with_capacity(limit);
        for row in rows {
            assets.push(serde_json::from_str(&row?)?);
        }
        Ok((assets, usize::try_from(total).unwrap_or(usize::MAX)))
    }

    /// 保存不可变修订；相同 ID 已存在时不覆盖。
    pub fn save_revision(&self, revision: &AssetRevision) -> StoreResult<()> {
        ensure_revision_secret_safe(revision)?;
        let json = serde_json::to_string(revision)?;
        let changed = self.connection.execute(
            "INSERT OR IGNORE INTO revisions(id, raw_hash, normalized_hash, content_object, json, created_at_ms)\
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                revision.id,
                revision.raw_hash.as_str(),
                revision.normalized_hash.as_str(),
                revision.content_object.as_str(),
                json,
                revision.created_at_ms
            ],
        )?;
        if changed == 0 {
            let existing: Option<String> = self
                .connection
                .query_row(
                    "SELECT json FROM revisions WHERE id=?1",
                    [revision.id.as_str()],
                    |row| row.get(0),
                )
                .optional()?;
            if existing.as_deref() != Some(json.as_str()) {
                return Err(StoreError::Integrity(format!(
                    "不可变 revision ID 已对应不同内容：{}",
                    revision.id
                )));
            }
        }
        Ok(())
    }

    /// 按 ID 读取不可变修订。
    pub fn load_revision(&self, id: &str) -> StoreResult<Option<AssetRevision>> {
        load_json(
            &self.connection,
            "SELECT json FROM revisions WHERE id=?1",
            id,
        )
    }

    /// 列出全部不可变修订。
    pub fn list_revisions(&self) -> StoreResult<Vec<AssetRevision>> {
        list_json(
            &self.connection,
            "SELECT json FROM revisions ORDER BY created_at_ms, id",
            [],
        )
    }

    /// 保存检测到的 Agent 实例。
    pub fn save_agent_instance(
        &self,
        instance: &AgentInstance,
        updated_at_ms: i64,
    ) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO agent_instances(id, adapter_id, json, updated_at_ms) VALUES(?1, ?2, ?3, ?4)\
             ON CONFLICT(id) DO UPDATE SET adapter_id=excluded.adapter_id, json=excluded.json, updated_at_ms=excluded.updated_at_ms",
            params![instance.id, instance.adapter_id, serde_json::to_string(instance)?, updated_at_ms],
        )?;
        Ok(())
    }

    /// 列出全部已检测实例，顺序按稳定 ID。
    pub fn list_agent_instances(&self) -> StoreResult<Vec<AgentInstance>> {
        list_json(
            &self.connection,
            "SELECT json FROM agent_instances ORDER BY id",
            [],
        )
    }

    /// 按 ID 读取 Agent 实例。
    pub fn load_agent_instance(&self, id: &str) -> StoreResult<Option<AgentInstance>> {
        load_json(
            &self.connection,
            "SELECT json FROM agent_instances WHERE id=?1",
            id,
        )
    }

    /// 保存分配。
    pub fn save_assignment(&self, assignment: &Assignment, updated_at_ms: i64) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO assignments(id, asset_id, revision_id, agent_instance_id, scope, json, updated_at_ms)\
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)\
             ON CONFLICT(id) DO UPDATE SET revision_id=excluded.revision_id, scope=excluded.scope, json=excluded.json, updated_at_ms=excluded.updated_at_ms",
            params![assignment.id, assignment.asset_id, assignment.revision_id, assignment.agent_instance_id, assignment.scope, serde_json::to_string(assignment)?, updated_at_ms],
        )?;
        Ok(())
    }

    /// 列出全部分配。
    pub fn list_assignments(&self) -> StoreResult<Vec<Assignment>> {
        list_json(
            &self.connection,
            "SELECT json FROM assignments ORDER BY id",
            [],
        )
    }

    /// 原子替换一个 Agent 实例的刷新库存，并同时保存新冲突、刷新运行和审计事件。
    pub fn commit_refresh(
        &mut self,
        agent_instance_id: &str,
        report: &RefreshReport,
        observed: &[ObservedFile],
    ) -> StoreResult<Vec<InventoryRecord>> {
        let mut ownership = BTreeMap::new();
        for snapshot in self.list_snapshots()? {
            let assignment_id = self
                .load_plan(&snapshot.plan_id)?
                .and_then(|plan| plan.assignment_id);
            for (path, hash) in snapshot.target_hashes {
                ownership.insert(path, (Some(hash), assignment_id.clone()));
            }
            for path in snapshot.removed_targets {
                ownership.insert(path, (None, assignment_id.clone()));
            }
        }
        let observed_by_path: BTreeMap<_, _> =
            observed.iter().map(|item| (&item.path, item)).collect();
        let mut reversible_assignments = BTreeSet::new();
        for assignment in self.list_assignments()? {
            let Some(asset) = self.load_asset(&assignment.asset_id)? else {
                continue;
            };
            let Some(instance) = self.load_agent_instance(&assignment.agent_instance_id)? else {
                continue;
            };
            if instance.surfaces.iter().any(|surface| {
                surface.scope == assignment.scope
                    && surface.asset_kind == asset.kind
                    && matches!(
                        surface.mode,
                        SurfaceMode::ReplaceFile | SurfaceMode::DirectoryTree
                    )
            }) {
                reversible_assignments.insert(assignment.id);
            }
        }
        let records: Vec<_> = report
            .items
            .iter()
            .cloned()
            .map(|mut item| {
                let observed = observed_by_path.get(&item.path).copied();
                if let Some(conflict) = &mut item.conflict {
                    conflict.agent_instance_id = Some(agent_instance_id.to_owned());
                    conflict.current_hash = observed.map(|value| value.raw_hash.clone());
                    if let Some((baseline_hash, assignment_id)) = ownership.get(&item.path) {
                        conflict.baseline_hash = baseline_hash.clone();
                        conflict.assignment_id = assignment_id.clone();
                        if !assignment_id
                            .as_ref()
                            .is_some_and(|id| reversible_assignments.contains(id))
                        {
                            conflict.actions.retain(|action| {
                                !matches!(
                                    action,
                                    ResolutionAction::ImportAgentRevision
                                        | ResolutionAction::RenameAndCoexist
                                )
                            });
                        }
                    }
                }
                InventoryRecord::from_refresh(
                    agent_instance_id,
                    item,
                    observed,
                    report.finished_at_ms,
                )
            })
            .collect();
        let run_id = ContentHash::from_bytes(
            format!(
                "refresh\0{agent_instance_id}\0{}\0{}",
                report.started_at_ms, report.finished_at_ms
            )
            .as_bytes(),
        )
        .to_string();
        let audit_id = ContentHash::from_bytes(format!("audit\0{run_id}").as_bytes()).to_string();
        let audit = AuditEvent {
            id: audit_id,
            event_type: "refresh_completed".to_owned(),
            plan_id: None,
            details: serde_json::json!({
                "agent_instance_id": agent_instance_id,
                "counts": report.counts,
                "refresh_run_id": run_id,
            }),
            created_at_ms: report.finished_at_ms,
        };

        let transaction = self.connection.transaction()?;
        transaction.execute(
            "DELETE FROM inventory_entries WHERE agent_instance_id=?1",
            [agent_instance_id],
        )?;
        for record in &records {
            transaction.execute(
                "INSERT INTO inventory_entries(id, agent_instance_id, path, state, json, updated_at_ms)\
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    record.id,
                    record.agent_instance_id,
                    record.item.path.as_str(),
                    drift_name(record.item.state),
                    serde_json::to_string(record)?,
                    record.updated_at_ms
                ],
            )?;
            if let Some(conflict) = &record.item.conflict {
                transaction.execute(
                    "INSERT INTO conflicts(id, kind, resolved, json, updated_at_ms) VALUES(?1, ?2, ?3, ?4, ?5)\
                     ON CONFLICT(id) DO UPDATE SET resolved=excluded.resolved, json=excluded.json, updated_at_ms=excluded.updated_at_ms",
                    params![
                        conflict.id,
                        format!("{:?}", conflict.kind),
                        conflict.resolved,
                        serde_json::to_string(conflict)?,
                        report.finished_at_ms
                    ],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO refresh_runs(id, agent_instance_id, json, started_at_ms, finished_at_ms)\
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                run_id,
                agent_instance_id,
                serde_json::to_string(report)?,
                report.started_at_ms,
                report.finished_at_ms
            ],
        )?;
        transaction.execute(
            "INSERT INTO audit_events(id, event_type, plan_id, json, created_at_ms) VALUES(?1, ?2, NULL, ?3, ?4)",
            params![
                audit.id,
                audit.event_type,
                serde_json::to_string(&audit)?,
                audit.created_at_ms
            ],
        )?;
        transaction.commit()?;
        Ok(records)
    }

    /// 读取某实例的当前库存。
    pub fn list_inventory(&self, agent_instance_id: &str) -> StoreResult<Vec<InventoryRecord>> {
        list_json(
            &self.connection,
            "SELECT json FROM inventory_entries WHERE agent_instance_id=?1 ORDER BY path",
            [agent_instance_id],
        )
    }

    /// 保存待应用计划；显式重新生成相同语义计划时重新打开为 `pending`。
    ///
    /// 计划 ID 由语义内容确定。一次“停用 → 启用 → 再停用”会合法地产生同一 ID；
    /// 只有再次调用 Planner 并保存，才允许从 applied 回到 pending。单纯重复 apply
    /// 不经过这里，仍会被服务层拒绝。
    pub fn save_plan(&self, plan: &DeploymentPlan) -> StoreResult<()> {
        ensure_plan_secret_safe(plan)?;
        self.connection.execute(
            "INSERT INTO plans(id, schema_version, status, json, created_at_ms) VALUES(?1, ?2, 'pending', ?3, ?4)\
             ON CONFLICT(id) DO UPDATE SET schema_version=excluded.schema_version, status='pending', json=excluded.json, created_at_ms=excluded.created_at_ms",
            params![plan.id, plan.schema_version, serde_json::to_string(plan)?, plan.created_at_ms],
        )?;
        Ok(())
    }

    /// 读取计划。
    pub fn load_plan(&self, id: &str) -> StoreResult<Option<DeploymentPlan>> {
        load_json(&self.connection, "SELECT json FROM plans WHERE id=?1", id)
    }

    /// 按可选状态列出计划。
    pub fn list_plans(&self, status: Option<&str>) -> StoreResult<Vec<DeploymentPlan>> {
        match status {
            Some(status) => list_json(
                &self.connection,
                "SELECT json FROM plans WHERE status=?1 ORDER BY created_at_ms DESC, id",
                [status],
            ),
            None => list_json(
                &self.connection,
                "SELECT json FROM plans ORDER BY created_at_ms DESC, id",
                [],
            ),
        }
    }

    /// 读取计划状态。
    pub fn plan_status(&self, id: &str) -> StoreResult<Option<String>> {
        Ok(self
            .connection
            .query_row("SELECT status FROM plans WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// 把待应用计划标记为失效或放弃。
    pub fn set_plan_status(&self, id: &str, status: &str) -> StoreResult<()> {
        if !matches!(status, "pending" | "applied" | "invalid" | "abandoned") {
            return Err(StoreError::Integrity(format!("计划状态无效：{status}")));
        }
        let changed = self.connection.execute(
            "UPDATE plans SET status=?2 WHERE id=?1",
            params![id, status],
        )?;
        if changed == 0 {
            return Err(StoreError::Integrity(format!("计划不存在：{id}")));
        }
        Ok(())
    }

    /// 在一个 SQLite transaction 中提交快照、审计和计划状态。
    ///
    /// 需要 `&mut self`，因为 rusqlite 的 transaction 独占借用 Connection；在该
    /// transaction 结束前，编译器会阻止其他查询同时使用同一连接。
    pub fn commit_apply(
        &mut self,
        plan: &DeploymentPlan,
        report: &ApplyReport,
        created_at_ms: i64,
    ) -> StoreResult<(DeploymentSnapshot, AuditEvent)> {
        let mut linked_conflicts: Vec<_> = self
            .list_conflicts(Some(false))?
            .into_iter()
            .filter(|conflict| conflict.resolution_plan_id.as_deref() == Some(&plan.id))
            .map(|conflict| {
                let action = conflict.selected_action.clone();
                (conflict, action)
            })
            .collect();
        for (conflict, action) in &mut linked_conflicts {
            if matches!(action, Some(ResolutionAction::AbandonPlan)) {
                // “放弃计划”只撤销本次解决尝试，不能谎称磁盘冲突已经消失。
                conflict.selected_action = None;
                conflict.resolution_plan_id = None;
            } else {
                conflict.resolved = true;
                conflict.resolved_at_ms = Some(created_at_ms);
            }
        }
        let target_hashes: BTreeMap<_, _> = report
            .files
            .iter()
            .filter_map(|file| {
                file.resulting_hash
                    .clone()
                    .map(|hash| (file.target_path.clone(), hash))
            })
            .collect();
        let removed_targets = report
            .files
            .iter()
            .filter(|file| file.resulting_hash.is_none())
            .map(|file| file.target_path.clone())
            .collect();
        let snapshot = DeploymentSnapshot {
            id: ContentHash::from_bytes(
                format!("snapshot\0{}\0{created_at_ms}", plan.id).as_bytes(),
            )
            .to_string(),
            plan_id: plan.id.clone(),
            target_hashes,
            removed_targets,
            created_at_ms,
        };
        let event = AuditEvent {
            id: ContentHash::from_bytes(format!("audit\0{}\0{created_at_ms}", plan.id).as_bytes())
                .to_string(),
            event_type: "plan_applied".to_owned(),
            plan_id: Some(plan.id.clone()),
            details: serde_json::to_value(report)?,
            created_at_ms,
        };

        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO deployment_snapshots(id, plan_id, json, created_at_ms) VALUES(?1, ?2, ?3, ?4)",
            params![snapshot.id, snapshot.plan_id, serde_json::to_string(&snapshot)?, created_at_ms],
        )?;
        transaction.execute(
            "INSERT INTO audit_events(id, event_type, plan_id, json, created_at_ms) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![event.id, event.event_type, event.plan_id, serde_json::to_string(&event)?, created_at_ms],
        )?;
        transaction.execute("UPDATE plans SET status='applied' WHERE id=?1", [&plan.id])?;
        if let Some(assignment_id) = &plan.assignment_id {
            let json: Option<String> = transaction
                .query_row(
                    "SELECT json FROM assignments WHERE id=?1",
                    [assignment_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(json) = json {
                let mut assignment: Assignment = serde_json::from_str(&json)?;
                assignment.enabled = !matches!(plan.purpose, rigdeck_core::PlanPurpose::Remove);
                transaction.execute(
                    "UPDATE assignments SET json=?2, updated_at_ms=?3 WHERE id=?1",
                    params![
                        assignment_id,
                        serde_json::to_string(&assignment)?,
                        created_at_ms
                    ],
                )?;
            }
        }
        // 文件已经原子落盘并通过验证；目录元数据必须在同一个 SQLite transaction
        // 中跟进。任何一条 SQL 失败都会回滚整个 transaction，随后 TransactionEngine
        // 会使用 rollback object 恢复文件，从而避免“文件新、数据库旧”的半成功状态。
        for effect in &plan.catalog_effects {
            match effect {
                CatalogEffect::SetAssetRevision {
                    asset_id,
                    revision_id,
                } => {
                    let json: String = transaction.query_row(
                        "SELECT json FROM assets WHERE id=?1",
                        [asset_id],
                        |row| row.get(0),
                    )?;
                    let mut asset: Asset = serde_json::from_str(&json)?;
                    asset.current_revision_id = Some(revision_id.clone());
                    transaction.execute(
                        "UPDATE assets SET json=?2, updated_at_ms=?3 WHERE id=?1",
                        params![asset_id, serde_json::to_string(&asset)?, created_at_ms],
                    )?;
                }
                CatalogEffect::SetAssignmentRevision {
                    assignment_id,
                    revision_id,
                } => {
                    let json: String = transaction.query_row(
                        "SELECT json FROM assignments WHERE id=?1",
                        [assignment_id],
                        |row| row.get(0),
                    )?;
                    let mut assignment: Assignment = serde_json::from_str(&json)?;
                    assignment.revision_id = revision_id.clone();
                    transaction.execute(
                        "UPDATE assignments SET revision_id=?2, json=?3, updated_at_ms=?4 WHERE id=?1",
                        params![assignment_id, revision_id, serde_json::to_string(&assignment)?, created_at_ms],
                    )?;
                }
                CatalogEffect::UpsertAsset { asset } => {
                    transaction.execute(
                        "INSERT INTO assets(id, kind, json, updated_at_ms) VALUES(?1, ?2, ?3, ?4)\
                         ON CONFLICT(id) DO UPDATE SET kind=excluded.kind, json=excluded.json, updated_at_ms=excluded.updated_at_ms",
                        params![asset.id, kind_name(asset.kind), serde_json::to_string(asset)?, created_at_ms],
                    )?;
                }
                CatalogEffect::UpsertAssignment { assignment } => {
                    transaction.execute(
                        "INSERT INTO assignments(id, asset_id, revision_id, agent_instance_id, scope, json, updated_at_ms)\
                         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)\
                         ON CONFLICT(id) DO UPDATE SET asset_id=excluded.asset_id, revision_id=excluded.revision_id, scope=excluded.scope, json=excluded.json, updated_at_ms=excluded.updated_at_ms",
                        params![
                            assignment.id,
                            assignment.asset_id,
                            assignment.revision_id,
                            assignment.agent_instance_id,
                            assignment.scope,
                            serde_json::to_string(assignment)?,
                            created_at_ms,
                        ],
                    )?;
                }
                CatalogEffect::AbandonPlan { plan_id } => {
                    let changed = transaction.execute(
                        "UPDATE plans SET status='abandoned' WHERE id=?1 AND status='pending'",
                        [plan_id],
                    )?;
                    if changed == 0 {
                        return Err(StoreError::Integrity(format!(
                            "待放弃计划不存在或不是 pending：{plan_id}"
                        )));
                    }
                }
            }
        }
        for (conflict, action) in &linked_conflicts {
            transaction.execute(
                "UPDATE conflicts SET resolved=1, json=?2, updated_at_ms=?3 WHERE id=?1",
                params![conflict.id, serde_json::to_string(conflict)?, created_at_ms],
            )?;
            let resolution_event = AuditEvent {
                id: ContentHash::from_bytes(
                    format!("audit\0conflict-resolved\0{}\0{created_at_ms}", conflict.id)
                        .as_bytes(),
                )
                .to_string(),
                event_type: if matches!(action, Some(ResolutionAction::AbandonPlan)) {
                    "conflict_resolution_abandoned".to_owned()
                } else {
                    "conflict_resolved".to_owned()
                },
                plan_id: Some(plan.id.clone()),
                details: serde_json::json!({
                    "conflict_id": conflict.id,
                    "action": action,
                }),
                created_at_ms,
            };
            transaction.execute(
                "INSERT INTO audit_events(id, event_type, plan_id, json, created_at_ms) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    resolution_event.id,
                    resolution_event.event_type,
                    resolution_event.plan_id,
                    serde_json::to_string(&resolution_event)?,
                    created_at_ms
                ],
            )?;
        }
        transaction.commit()?;
        Ok((snapshot, event))
    }

    /// 保存/更新冲突记录。
    pub fn save_conflict(&self, conflict: &Conflict, updated_at_ms: i64) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO conflicts(id, kind, resolved, json, updated_at_ms) VALUES(?1, ?2, ?3, ?4, ?5)\
             ON CONFLICT(id) DO UPDATE SET resolved=excluded.resolved, json=excluded.json, updated_at_ms=excluded.updated_at_ms",
            params![conflict.id, format!("{:?}", conflict.kind), conflict.resolved, serde_json::to_string(conflict)?, updated_at_ms],
        )?;
        Ok(())
    }

    /// 按 ID 读取冲突。
    pub fn load_conflict(&self, id: &str) -> StoreResult<Option<Conflict>> {
        load_json(
            &self.connection,
            "SELECT json FROM conflicts WHERE id=?1",
            id,
        )
    }

    /// 列出冲突；`resolved=None` 表示不过滤。
    pub fn list_conflicts(&self, resolved: Option<bool>) -> StoreResult<Vec<Conflict>> {
        match resolved {
            Some(value) => list_json(
                &self.connection,
                "SELECT json FROM conflicts WHERE resolved=?1 ORDER BY updated_at_ms DESC, id",
                [i64::from(value)],
            ),
            None => list_json(
                &self.connection,
                "SELECT json FROM conflicts ORDER BY updated_at_ms DESC, id",
                [],
            ),
        }
    }

    /// 把用户选择与显式计划关联；此时冲突仍未解决，必须等待计划成功应用。
    pub fn attach_conflict_plan(
        &mut self,
        id: &str,
        action: ResolutionAction,
        plan_id: &str,
        created_at_ms: i64,
    ) -> StoreResult<Conflict> {
        let mut conflict = self
            .load_conflict(id)?
            .ok_or_else(|| StoreError::Integrity(format!("冲突不存在：{id}")))?;
        if conflict.resolved {
            return Err(StoreError::Integrity(format!("冲突已经解决：{id}")));
        }
        if !conflict.actions.contains(&action) {
            return Err(StoreError::Integrity(format!(
                "动作 {action:?} 不是冲突 {id} 的合法下一步"
            )));
        }
        if self.plan_status(plan_id)?.as_deref() != Some("pending") {
            return Err(StoreError::Integrity(format!(
                "冲突解决计划不是 pending：{plan_id}"
            )));
        }
        let superseded_plan = conflict
            .resolution_plan_id
            .clone()
            .filter(|previous| previous != plan_id);
        conflict.selected_action = Some(action);
        conflict.resolution_plan_id = Some(plan_id.to_owned());
        let event = AuditEvent {
            id: ContentHash::from_bytes(
                format!("audit\0conflict-planned\0{id}\0{plan_id}\0{created_at_ms}").as_bytes(),
            )
            .to_string(),
            event_type: "conflict_resolution_planned".to_owned(),
            plan_id: Some(plan_id.to_owned()),
            details: serde_json::json!({
                "conflict_id": id,
                "action": conflict.selected_action,
            }),
            created_at_ms,
        };
        let transaction = self.connection.transaction()?;
        if let Some(previous) = superseded_plan {
            transaction.execute(
                "UPDATE plans SET status='abandoned' WHERE id=?1 AND status='pending'",
                [previous],
            )?;
        }
        transaction.execute(
            "UPDATE conflicts SET json=?2, updated_at_ms=?3 WHERE id=?1",
            params![id, serde_json::to_string(&conflict)?, created_at_ms],
        )?;
        transaction.execute(
            "INSERT INTO audit_events(id, event_type, plan_id, json, created_at_ms) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![event.id, event.event_type, event.plan_id, serde_json::to_string(&event)?, created_at_ms],
        )?;
        transaction.commit()?;
        Ok(conflict)
    }

    /// 记录用户选择的合法冲突动作；真正产生新基线的计划应用仍由 Planner 完成。
    pub fn resolve_conflict(
        &mut self,
        id: &str,
        action: ResolutionAction,
        resolved_at_ms: i64,
    ) -> StoreResult<Conflict> {
        let mut conflict = self
            .load_conflict(id)?
            .ok_or_else(|| StoreError::Integrity(format!("冲突不存在：{id}")))?;
        if !conflict.actions.contains(&action) {
            return Err(StoreError::Integrity(format!(
                "动作 {action:?} 不是冲突 {id} 的合法下一步"
            )));
        }
        conflict.resolved = true;
        let audit = AuditEvent {
            id: ContentHash::from_bytes(
                format!("audit\0conflict\0{id}\0{resolved_at_ms}").as_bytes(),
            )
            .to_string(),
            event_type: "conflict_resolution_selected".to_owned(),
            plan_id: None,
            details: serde_json::json!({
                "conflict_id": id,
                "action": action,
            }),
            created_at_ms: resolved_at_ms,
        };
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "UPDATE conflicts SET resolved=1, json=?2, updated_at_ms=?3 WHERE id=?1",
            params![id, serde_json::to_string(&conflict)?, resolved_at_ms],
        )?;
        transaction.execute(
            "INSERT INTO audit_events(id, event_type, plan_id, json, created_at_ms) VALUES(?1, ?2, NULL, ?3, ?4)",
            params![
                audit.id,
                audit.event_type,
                serde_json::to_string(&audit)?,
                audit.created_at_ms
            ],
        )?;
        transaction.commit()?;
        Ok(conflict)
    }

    /// 返回最近一次成功部署快照。
    pub fn latest_snapshot(&self) -> StoreResult<Option<DeploymentSnapshot>> {
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT json FROM deployment_snapshots ORDER BY created_at_ms DESC, id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|value| serde_json::from_str(&value).map_err(StoreError::from))
            .transpose()
    }

    /// 按时间列出全部部署快照，用于重建每个目标的最近基线。
    pub fn list_snapshots(&self) -> StoreResult<Vec<DeploymentSnapshot>> {
        list_json(
            &self.connection,
            "SELECT json FROM deployment_snapshots ORDER BY created_at_ms, id",
            [],
        )
    }

    /// 列出最近审计事件。
    pub fn list_audit_events(&self, limit: usize) -> StoreResult<Vec<AuditEvent>> {
        let limit = i64::try_from(limit.min(10_000)).unwrap_or(10_000);
        list_json(
            &self.connection,
            "SELECT json FROM audit_events ORDER BY created_at_ms DESC, id DESC LIMIT ?1",
            [limit],
        )
    }

    /// 执行 SQLite `integrity_check` 并验证 schema version。
    pub fn integrity_check(&self) -> StoreResult<()> {
        let status: String = self
            .connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if status != "ok" {
            return Err(StoreError::Integrity(status));
        }
        let version: u32 = self.connection.query_row(
            "SELECT value FROM rigdeck_meta WHERE key='schema_version'",
            [],
            |row| {
                row.get::<_, String>(0).and_then(|value| {
                    value.parse::<u32>().map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })
                })
            },
        )?;
        if version != STORE_SCHEMA_VERSION {
            return Err(StoreError::Integrity(format!(
                "schema version {version} != {STORE_SCHEMA_VERSION}"
            )));
        }
        Ok(())
    }

    /// 使用 SQLite backup API 生成一致性备份。
    pub fn backup(&self, destination: impl AsRef<Utf8Path>) -> StoreResult<()> {
        let destination = destination.as_ref();
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| StoreError::io(parent, error))?;
        }
        self.connection
            .backup(DatabaseName::Main, destination.as_std_path(), None)?;
        Ok(())
    }

    /// 用一致性 SQLite backup 恢复当前数据库。
    pub fn restore(&mut self, source: impl AsRef<Utf8Path>) -> StoreResult<()> {
        let source = source.as_ref();
        let source_connection = Connection::open(source)?;
        let status: String =
            source_connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if status != "ok" {
            return Err(StoreError::Integrity(format!(
                "备份数据库 integrity_check 失败：{status}"
            )));
        }
        drop(source_connection);
        self.connection.restore(
            DatabaseName::Main,
            source.as_std_path(),
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        self.integrity_check()
    }

    /// 追加一条不含 secret 的审计事件。
    pub fn save_audit_event(&self, event: &AuditEvent) -> StoreResult<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO audit_events(id, event_type, plan_id, json, created_at_ms) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                event.id,
                event.event_type,
                event.plan_id,
                serde_json::to_string(event)?,
                event.created_at_ms
            ],
        )?;
        Ok(())
    }
}

fn configure_and_migrate(connection: &mut Connection) -> StoreResult<()> {
    connection.execute_batch(
        "PRAGMA foreign_keys=ON;\nPRAGMA journal_mode=WAL;\nPRAGMA synchronous=FULL;",
    )?;
    // Refinery 0.8 在损坏的 checksum 行上会 panic，而不是返回 Error。外部数据库
    // 是不可信输入，因此必须在 crate 边界捕获第三方 panic，才能继续恢复保护副本。
    match catch_unwind(AssertUnwindSafe(|| {
        embedded::migrations::runner().run(connection)
    })) {
        Ok(result) => {
            result?;
        }
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("未知 Refinery panic");
            return Err(StoreError::Integrity(format!(
                "migration runner panic：{message}"
            )));
        }
    }
    Ok(())
}

fn remove_sqlite_sidecars(path: &Utf8Path) -> StoreResult<()> {
    for suffix in ["-wal", "-shm"] {
        let sidecar = Utf8PathBuf::from(format!("{path}{suffix}"));
        match fs::remove_file(&sidecar) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(StoreError::io(&sidecar, error)),
        }
    }
    Ok(())
}

fn load_json<T: serde::de::DeserializeOwned>(
    connection: &Connection,
    sql: &str,
    id: &str,
) -> StoreResult<Option<T>> {
    let json: Option<String> = connection
        .query_row(sql, [id], |row| row.get(0))
        .optional()?;
    json.map(|value| serde_json::from_str(&value).map_err(StoreError::from))
        .transpose()
}

fn list_json<T, P>(connection: &Connection, sql: &str, params: P) -> StoreResult<Vec<T>>
where
    T: serde::de::DeserializeOwned,
    P: rusqlite::Params,
{
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params, |row| row.get::<_, String>(0))?;
    let mut output = Vec::new();
    for row in rows {
        output.push(serde_json::from_str(&row?)?);
    }
    Ok(output)
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

fn kind_name(kind: rigdeck_core::AssetKind) -> &'static str {
    match kind {
        rigdeck_core::AssetKind::Skill => "skill",
        rigdeck_core::AssetKind::Prompt => "prompt",
        rigdeck_core::AssetKind::McpServer => "mcp_server",
    }
}

fn ensure_revision_secret_safe(revision: &AssetRevision) -> StoreResult<()> {
    let AssetSpec::McpServer(spec) = &revision.spec else {
        return Ok(());
    };
    let bindings = match &spec.transport {
        McpTransport::Stdio { env, .. } => env,
        McpTransport::StreamableHttp { headers, .. } => headers,
    };
    for (key, value) in bindings {
        let normalized = key.to_ascii_uppercase().replace('-', "_");
        let sensitive = ["TOKEN", "SECRET", "PASSWORD", "AUTHORIZATION", "API_KEY"]
            .iter()
            .any(|needle| normalized.contains(needle));
        if sensitive && matches!(value, BindingValue::Literal(_)) {
            return Err(StoreError::SecretPolicy(format!(
                "敏感 MCP 绑定 {key} 必须使用 SecretRef"
            )));
        }
    }
    Ok(())
}

fn ensure_plan_secret_safe(plan: &DeploymentPlan) -> StoreResult<()> {
    const MARKERS: &[&str] = &[
        "authorization: bearer ",
        "password=",
        "password:",
        "api_key=",
        "api-key:",
        "token=",
        "client_secret",
    ];
    for operation in &plan.operations {
        let lower = operation.rendered_diff.to_ascii_lowercase();
        if MARKERS.iter().any(|marker| lower.contains(marker)) {
            return Err(StoreError::SecretPolicy(format!(
                "计划 diff 疑似包含明文 secret：{}",
                operation.target_path
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rigdeck_core::{
        classify_refresh, AgentHealth, AssetIdentity, AssetKind, AssetState, AuditResult,
        BindingValue, McpServerSpec, McpTransport, ObservationIssue, ObservedFile, OperationKind,
        PlannedOperation, ResolutionAction, RiskLevel, Source, SourceKind,
    };

    use super::*;

    fn revision(binding: BindingValue) -> AssetRevision {
        let content = ContentHash::from_bytes(b"fixture");
        AssetRevision {
            id: "revision-1".to_owned(),
            raw_hash: content.clone(),
            normalized_hash: content.clone(),
            content_object: content,
            source: Source {
                kind: SourceKind::LocalFolder,
                namespace: "local".to_owned(),
                locator: "fixture".to_owned(),
                revision: None,
            },
            license: None,
            audit: AuditResult {
                schema_version: 1,
                completed: true,
                findings: Vec::new(),
            },
            spec: AssetSpec::McpServer(McpServerSpec {
                server_name: "demo".to_owned(),
                transport: McpTransport::StreamableHttp {
                    url: "https://example.invalid/mcp".to_owned(),
                    headers: BTreeMap::from([("Authorization".to_owned(), binding)]),
                },
                enabled: true,
                timeout_ms: Some(1_000),
                oauth: None,
                allowed_tools: Vec::new(),
                denied_tools: Vec::new(),
            }),
            created_at_ms: 1,
            author: None,
            update_time_ms: None,
            platform_restrictions: Vec::new(),
        }
    }

    #[test]
    fn database_round_trip_and_integrity() {
        let database = Database::in_memory().unwrap();
        let identity = AssetIdentity::new("local", "fixture", ".", "demo").unwrap();
        let asset = Asset {
            id: identity.stable_id(),
            identity,
            kind: AssetKind::Skill,
            display_name: "Demo".to_owned(),
            current_revision_id: None,
            state: AssetState::Active,
            tags: Vec::new(),
        };
        database.save_asset(&asset, 1).unwrap();
        assert_eq!(database.load_asset(&asset.id).unwrap(), Some(asset));
        database.integrity_check().unwrap();
    }

    #[test]
    fn ten_thousand_assets_are_counted_and_paged_inside_sqlite() {
        use std::time::{Duration, Instant};

        let database = Database::in_memory().unwrap();
        for index in 0..10_000 {
            let identity = AssetIdentity::new(
                if index % 2 == 0 {
                    "source-a"
                } else {
                    "source-b"
                },
                format!("package-{}", index % 10),
                ".",
                format!("asset-{index:05}"),
            )
            .unwrap();
            database
                .save_asset(&Asset::new(identity, AssetKind::Skill), index)
                .unwrap();
        }

        assert_eq!(database.count_assets().unwrap(), 10_000);
        let mut samples = Vec::with_capacity(20);
        for _ in 0..20 {
            let started = Instant::now();
            let (page, total) = database
                .list_assets_page(Some("skill"), "", "", 9_950, 50)
                .unwrap();
            samples.push(started.elapsed());
            assert_eq!(total, 10_000);
            assert_eq!(page.len(), 50);
        }
        samples.sort_unstable();
        let page_elapsed = samples[18];
        eprintln!("10k 库存分页查询 P95：{page_elapsed:?}");
        assert!(
            page_elapsed < Duration::from_millis(250),
            "10k 库存分页查询 P95 耗时 {page_elapsed:?}"
        );

        let (matches, total) = database
            .list_assets_page(Some("skill"), "asset-099", "source-a", 0, 50)
            .unwrap();
        assert_eq!(total, 50);
        assert_eq!(matches.len(), 50);
        assert!(matches
            .iter()
            .all(|asset| asset.identity.declared_name.starts_with("asset-099")));

        // `%` 是普通搜索字符，不得被当成 SQL LIKE 通配符。
        let (wildcard, total) = database.list_assets_page(None, "%", "", 0, 50).unwrap();
        assert_eq!(total, 0);
        assert!(wildcard.is_empty());
    }

    #[test]
    fn plaintext_sensitive_binding_is_rejected() {
        let database = Database::in_memory().unwrap();
        let error = database
            .save_revision(&revision(BindingValue::Literal("Bearer value".to_owned())))
            .unwrap_err();
        assert!(matches!(error, StoreError::SecretPolicy(_)));
    }

    #[test]
    fn secret_ref_binding_is_persisted() {
        let database = Database::in_memory().unwrap();
        database
            .save_revision(&revision(BindingValue::Secret(
                rigdeck_core::SecretRef::new("keychain:mcp-demo").unwrap(),
            )))
            .unwrap();
    }

    #[test]
    fn plan_diff_with_plaintext_secret_is_rejected() {
        let database = Database::in_memory().unwrap();
        let hash = ContentHash::from_bytes(b"desired");
        let plan = DeploymentPlan {
            schema_version: 1,
            id: "plan-secret".to_owned(),
            assignment_id: None,
            purpose: rigdeck_core::PlanPurpose::Install,
            source_hashes: Vec::new(),
            operations: vec![PlannedOperation {
                id: "operation".to_owned(),
                kind: OperationKind::WriteFile,
                target_path: Utf8PathBuf::from("/tmp/config"),
                expected_target_hash: None,
                desired_object: Some(hash.clone()),
                desired_hash: Some(hash),
                rollback_object: None,
                rendered_diff: "Authorization: Bearer TEST_SECRET".to_owned(),
                compatibility_losses: Vec::new(),
                risk: RiskLevel::High,
            }],
            catalog_effects: Vec::new(),
            risk: RiskLevel::High,
            created_at_ms: 1,
        };
        assert!(matches!(
            database.save_plan(&plan),
            Err(StoreError::SecretPolicy(_))
        ));
    }

    #[test]
    fn refresh_inventory_conflict_and_resolution_are_transactional() {
        let mut database = Database::in_memory().unwrap();
        let instance = AgentInstance {
            id: "agent-fixture".to_owned(),
            adapter_id: "fixture".to_owned(),
            display_name: "Fixture".to_owned(),
            version: None,
            managed_roots: vec![Utf8PathBuf::from("/fixture")],
            profile: None,
            health: AgentHealth::Healthy,
            surfaces: Vec::new(),
        };
        database.save_agent_instance(&instance, 1).unwrap();
        let observed = vec![ObservedFile {
            path: Utf8PathBuf::from("/fixture/AGENTS.md"),
            raw_hash: ContentHash::from_bytes(b"broken"),
            normalized_hash: ContentHash::from_bytes(b"broken"),
            logical_id: Some("prompt-1".to_owned()),
            issue: Some(ObservationIssue::DamagedManagedBlock),
        }];
        let report = classify_refresh(&[], &observed, &std::collections::BTreeSet::new(), 10, 20);
        let records = database
            .commit_refresh(&instance.id, &report, &observed)
            .unwrap();
        assert_eq!(records, database.list_inventory(&instance.id).unwrap());
        let conflicts = database.list_conflicts(Some(false)).unwrap();
        assert_eq!(conflicts.len(), 1);
        let resolved = database
            .resolve_conflict(&conflicts[0].id, ResolutionAction::ImportAgentRevision, 30)
            .unwrap();
        assert!(resolved.resolved);
        assert!(database.list_conflicts(Some(false)).unwrap().is_empty());
        assert_eq!(database.list_conflicts(Some(true)).unwrap().len(), 1);
    }

    #[test]
    fn failed_migration_restores_pre_migration_database() {
        let temp = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(temp.path().join("rigdeck.sqlite3")).unwrap();
        let database = Database::open(path.clone()).unwrap();
        let identity = AssetIdentity::new("local", "fixture", ".", "demo").unwrap();
        let asset = Asset::new(identity, AssetKind::Skill);
        database.save_asset(&asset, 1).unwrap();
        drop(database);

        // 伪造 migration checksum，使 refinery 在下一次打开时明确失败。
        let connection = Connection::open(&path).unwrap();
        connection
            .execute("UPDATE refinery_schema_history SET checksum='broken'", [])
            .unwrap();
        drop(connection);

        assert!(Database::open(path.clone()).is_err());
        let connection = Connection::open(path).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
