//! Adapter 投影到 Core Planner 的物化桥接层。
//!
//! Adapter 渲染的是“受管理片段”，本模块读取当前目标并执行通用保真合成，最后把
//! 完整目标字节交给 Planner。读取不会修改文件；备份、原子写入、验证、回滚与审计
//! 仍全部由 Core 事务引擎负责。

use std::fs;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use rigdeck_adapter_sdk::{
    AdapterError, AdapterErrorCode, AdapterResult, RemovalIntent, RemovalStrategy, RenderOutput,
};
use rigdeck_core::{
    AgentInstance, ContentHash, ContentStore, Planner, Projection, ProjectionStrategy, RiskLevel,
};

use crate::{
    remove_managed_block, remove_structured_entry, upsert_managed_block, upsert_structured_entry,
};

/// 把 Adapter 新生成的对象写入内容寻址库，并验证对象库返回同一 hash。
pub fn persist_rendered_objects(
    output: &RenderOutput,
    objects: &dyn ContentStore,
) -> AdapterResult<()> {
    for rendered in &output.objects {
        if ContentHash::from_bytes(&rendered.bytes) != rendered.hash {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                format!("Adapter rendered object hash 不匹配：{}", rendered.hash),
            ));
        }
        let stored = objects.put(&rendered.bytes).map_err(core_error)?;
        if stored != rendered.hash {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "对象库存储后返回了不同 hash",
            ));
        }
    }
    Ok(())
}

/// 将一个投影无副作用地合成为文件级 Planner 操作。
pub fn add_projection_to_planner(
    planner: &mut Planner<'_>,
    instance: &AgentInstance,
    projection: &Projection,
    objects: &dyn ContentStore,
) -> AdapterResult<()> {
    if projection.agent_instance_id != instance.id {
        return Err(AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            "Projection 与 AgentInstance 不匹配",
        ));
    }
    for file in &projection.files {
        ensure_inside_instance(instance, &file.target_path)?;
        let fragment = objects.get(&file.content_object).map_err(core_error)?;
        if ContentHash::from_bytes(&fragment) != file.raw_hash {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                format!("投影片段 hash 不一致：{}", file.target_path),
            ));
        }

        let (target, desired, risk, summary) = match &file.strategy {
            ProjectionStrategy::ReplaceFile => (
                file.target_path.clone(),
                fragment,
                existing_risk(&file.target_path),
                "整体替换 Adapter 独占文件".to_owned(),
            ),
            ProjectionStrategy::DirectoryTree { relative_path } => {
                validate_relative(relative_path, "Skill relative path")?;
                let target = file.target_path.join(relative_path);
                ensure_inside(&target, &file.target_path)?;
                (
                    target.clone(),
                    fragment,
                    existing_risk(&target),
                    format!("写入 Skill 文件 {relative_path}"),
                )
            }
            ProjectionStrategy::ManagedBlock { block_id } => {
                let current = read_optional(&file.target_path)?;
                let desired = upsert_managed_block(
                    current.as_deref().unwrap_or_default(),
                    block_id,
                    &projection.revision_id,
                    &fragment,
                )?;
                (
                    file.target_path.clone(),
                    desired,
                    RiskLevel::Medium,
                    format!("创建或更新托管块 {block_id}"),
                )
            }
            ProjectionStrategy::StructuredEntry { section, entry_key } => {
                let current = read_optional(&file.target_path)?;
                let desired = upsert_structured_entry(
                    current.as_deref().unwrap_or_default(),
                    &file.native_format,
                    section,
                    entry_key,
                    &fragment,
                )?;
                (
                    file.target_path.clone(),
                    desired,
                    RiskLevel::Medium,
                    format!("创建或更新结构化条目 {section}.{entry_key}"),
                )
            }
        };
        planner
            .write_file(
                target,
                &desired,
                summary,
                projection.compatibility_losses.clone(),
                risk,
            )
            .map_err(core_error)?;
    }
    Ok(())
}

/// 把精确卸载意图转换为 Planner 操作。
///
/// `owned_files` 来自最近一次成功 DeploymentSnapshot。目录型资产只删除快照证明属于
/// 该资产的文件；目录中后来出现的未知文件不会被顺带删除。
pub fn add_removals_to_planner(
    planner: &mut Planner<'_>,
    instance: &AgentInstance,
    removals: &[RemovalIntent],
    owned_files: &[Utf8PathBuf],
) -> AdapterResult<()> {
    for removal in removals {
        ensure_inside_instance(instance, &removal.target_path)?;
        match &removal.strategy {
            RemovalStrategy::RemoveOwnedPath => {
                if removal.target_path.is_file() {
                    planner
                        .remove_file(
                            removal.target_path.clone(),
                            "删除 Adapter 独占文件",
                            RiskLevel::High,
                        )
                        .map_err(core_error)?;
                } else if removal.target_path.is_dir() {
                    let mut matched = Vec::new();
                    for owned in owned_files {
                        ensure_inside(owned, &removal.target_path)?;
                        if owned.is_file() {
                            matched.push(owned.clone());
                        }
                    }
                    if matched.is_empty() {
                        return Err(AdapterError::new(
                            AdapterErrorCode::ValidationFailed,
                            format!(
                                "目录卸载缺少 DeploymentSnapshot 文件清单：{}",
                                removal.target_path
                            ),
                        )
                        .with_recovery("先运行 refresh/doctor 恢复基线，或逐文件人工确认"));
                    }
                    // 先按路径倒序删除深层文件，虽然 Planner 最终会确定性排序，但文件
                    // 操作本身不删除目录，因此顺序不会误删未知内容。
                    matched.sort_by(|left, right| right.cmp(left));
                    for target in matched {
                        planner
                            .remove_file(target, "删除快照证明属于 Skill 的文件", RiskLevel::High)
                            .map_err(core_error)?;
                    }
                }
            }
            RemovalStrategy::ManagedBlock { block_id } => {
                let Some(current) = read_optional(&removal.target_path)? else {
                    continue;
                };
                let desired = remove_managed_block(&current, block_id)?;
                planner
                    .write_file(
                        removal.target_path.clone(),
                        &desired,
                        format!("只删除托管块 {block_id}"),
                        Vec::new(),
                        RiskLevel::High,
                    )
                    .map_err(core_error)?;
            }
            RemovalStrategy::StructuredEntry { section, entry_key } => {
                let Some(current) = read_optional(&removal.target_path)? else {
                    continue;
                };
                let native_format = native_format_for_path(&removal.target_path)?;
                let desired = remove_structured_entry(&current, native_format, section, entry_key)?;
                planner
                    .write_file(
                        removal.target_path.clone(),
                        &desired,
                        format!("只删除结构化条目 {section}.{entry_key}"),
                        Vec::new(),
                        RiskLevel::High,
                    )
                    .map_err(core_error)?;
            }
        }
    }
    Ok(())
}

fn ensure_inside_instance(instance: &AgentInstance, path: &Utf8Path) -> AdapterResult<()> {
    if instance
        .managed_roots
        .iter()
        .any(|root| path.starts_with(root))
    {
        Ok(())
    } else {
        Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("投影目标不在实例允许根目录：{path}"),
        ))
    }
}

fn ensure_inside(path: &Utf8Path, root: &Utf8Path) -> AdapterResult<()> {
    if path.starts_with(root)
        && !path
            .components()
            .any(|part| matches!(part, Utf8Component::ParentDir))
    {
        Ok(())
    } else {
        Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("路径逃逸允许根目录：{path}"),
        ))
    }
}

fn validate_relative(path: &Utf8Path, label: &str) -> AdapterResult<()> {
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Utf8Component::ParentDir | Utf8Component::RootDir | Utf8Component::Prefix(_)
            )
        })
    {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("{label} 必须是不能逃逸的相对路径：{path}"),
        ));
    }
    Ok(())
}

fn read_optional(path: &Utf8Path) -> AdapterResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AdapterError::new(
            AdapterErrorCode::Io,
            format!("无法读取目标 {path}：{error}"),
        )),
    }
}

fn existing_risk(path: &Utf8Path) -> RiskLevel {
    if path.exists() {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

fn native_format_for_path(path: &Utf8Path) -> AdapterResult<&'static str> {
    match path
        .extension()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => Ok("json"),
        "jsonc" => Ok("jsonc"),
        "toml" => Ok("toml"),
        "yaml" | "yml" => Ok("yaml"),
        extension => Err(AdapterError::new(
            AdapterErrorCode::UnsupportedCapability,
            format!("无法从扩展名推断结构化格式：{extension}"),
        )),
    }
}

fn core_error(error: rigdeck_core::CoreError) -> AdapterError {
    AdapterError::new(AdapterErrorCode::Core, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeMap};

    use rigdeck_core::{
        ContentStore, CoreError, CoreResult, NoFailure, ProjectedFile, TransactionEngine,
    };

    use super::*;

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

    fn instance(root: &Utf8Path) -> AgentInstance {
        AgentInstance {
            id: "instance".to_owned(),
            adapter_id: "fixture".to_owned(),
            display_name: "Fixture".to_owned(),
            version: None,
            managed_roots: vec![root.to_owned()],
            profile: None,
            health: rigdeck_core::AgentHealth::Healthy,
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn managed_block_materialization_then_transaction_preserves_user_content() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let target = root.join("AGENTS.md");
        fs::write(&target, b"# User\r\nkeep\r\n").unwrap();
        let objects = MemoryObjects::default();
        let fragment = objects.put(b"managed content").unwrap();
        let projection = Projection {
            revision_id: "rev-1".to_owned(),
            agent_instance_id: "instance".to_owned(),
            files: vec![ProjectedFile {
                target_path: target.clone(),
                content_object: fragment.clone(),
                raw_hash: fragment,
                native_format: "markdown".to_owned(),
                strategy: ProjectionStrategy::ManagedBlock {
                    block_id: "asset-1".to_owned(),
                },
            }],
            compatibility_losses: Vec::new(),
        };
        let mut planner = Planner::new(None, Vec::new(), &objects);
        add_projection_to_planner(&mut planner, &instance(&root), &projection, &objects).unwrap();
        let plan = planner.finish(0).unwrap();
        TransactionEngine
            .apply(&plan, &objects, &NoFailure, |_| Ok(()))
            .unwrap();
        let result = fs::read(&target).unwrap();
        assert!(result.starts_with(b"# User\r\nkeep\r\n"));
        assert!(String::from_utf8_lossy(&result).contains("asset=asset-1 revision=rev-1"));
    }

    #[test]
    fn structured_materialization_preserves_jsonc_user_fields() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let target = root.join("opencode.jsonc");
        fs::write(&target, b"{ // user\n  \"theme\": \"dark\"\n}\n").unwrap();
        let objects = MemoryObjects::default();
        let payload = br#"{"type":"local","command":["demo"]}"#;
        let fragment = objects.put(payload).unwrap();
        let projection = Projection {
            revision_id: "rev-mcp".to_owned(),
            agent_instance_id: "instance".to_owned(),
            files: vec![ProjectedFile {
                target_path: target.clone(),
                content_object: fragment.clone(),
                raw_hash: fragment,
                native_format: "jsonc".to_owned(),
                strategy: ProjectionStrategy::StructuredEntry {
                    section: "mcp".to_owned(),
                    entry_key: "demo".to_owned(),
                },
            }],
            compatibility_losses: Vec::new(),
        };
        let mut planner = Planner::new(None, Vec::new(), &objects);
        add_projection_to_planner(&mut planner, &instance(&root), &projection, &objects).unwrap();
        let plan = planner.finish(0).unwrap();
        TransactionEngine
            .apply(&plan, &objects, &NoFailure, |_| Ok(()))
            .unwrap();
        let result = fs::read_to_string(target).unwrap();
        assert!(result.contains("// user"));
        assert!(result.contains("\"theme\": \"dark\""));
        assert!(result.contains("\"demo\""));
    }

    #[test]
    fn projection_outside_instance_root_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join("inside")).unwrap();
        let outside = Utf8PathBuf::from_path_buf(temp.path().join("outside.txt")).unwrap();
        fs::create_dir_all(&root).unwrap();
        let objects = MemoryObjects::default();
        let fragment = objects.put(b"value").unwrap();
        let projection = Projection {
            revision_id: "rev".to_owned(),
            agent_instance_id: "instance".to_owned(),
            files: vec![ProjectedFile {
                target_path: outside,
                content_object: fragment.clone(),
                raw_hash: fragment,
                native_format: "text".to_owned(),
                strategy: ProjectionStrategy::ReplaceFile,
            }],
            compatibility_losses: Vec::new(),
        };
        let mut planner = Planner::new(None, Vec::new(), &objects);
        let error =
            add_projection_to_planner(&mut planner, &instance(&root), &projection, &objects)
                .unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::PathViolation);
    }
}
