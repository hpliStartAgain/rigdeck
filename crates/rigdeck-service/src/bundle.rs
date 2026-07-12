//! 不含明文凭据的可移植导入/导出包。

use std::{fs, io::Write};

use base64::{engine::general_purpose::STANDARD, Engine};
use camino::{Utf8Path, Utf8PathBuf};
use rigdeck_adapter_sdk::{AssetContent, AssetFileContent};
use rigdeck_core::{Asset, AssetRevision, AssetSpec, Assignment, ContentHash};
use rigdeck_registry::{persist_imported_skill, validate_asset_content, ImportedSkill};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{now_ms, RigDeckService, ServiceError, ServiceResult};

const MAX_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;

/// 一个经过 base64 编码的资产文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableFile {
    /// 资产根内的相对路径。
    pub path: Utf8PathBuf,
    /// 标准 base64；不会在日志中单独展开。
    pub content_base64: String,
    /// 来源可执行位，仅作为 metadata 保存，不会自动执行。
    pub executable: bool,
}

/// 一个资产和当前不可变修订。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableAsset {
    /// 逻辑资产。
    pub asset: Asset,
    /// 当前修订，Secret 只可能以 `SecretRef` 出现。
    pub revision: AssetRevision,
    /// 完整文件树。
    pub files: Vec<PortableFile>,
}

/// 跨机器分配意图，不保存机器专属的 Agent instance ID。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableAssignment {
    /// 资产 ID。
    pub asset_id: String,
    /// Adapter ID。
    pub adapter_id: String,
    /// 可选 profile。
    pub profile: Option<String>,
    /// 原生作用域。
    pub scope: String,
    /// 是否启用。
    pub enabled: bool,
}

/// RigDeck 可移植包；不包含计划、备份、审计和任何钥匙串值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableBundle {
    /// 包 schema。
    pub schema_version: u32,
    /// 创建时间。
    pub created_at_ms: i64,
    /// 资产与内容。
    pub assets: Vec<PortableAsset>,
    /// 可在目标机器重新匹配的分配意图。
    pub assignments: Vec<PortableAssignment>,
}

/// 一次导入结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleImportReport {
    /// 导入的资产 ID。
    pub asset_ids: Vec<String>,
    /// 因目标机器没有唯一 Agent 实例而未自动恢复的分配意图。
    pub pending_assignments: Vec<PortableAssignment>,
}

impl RigDeckService {
    /// 原子导出配置包。对象库密钥、HTTP token 和钥匙串值都不在领域模型中。
    pub fn export_bundle(&self, output: &Utf8Path) -> ServiceResult<PortableBundle> {
        let mut assets = Vec::new();
        for asset in self.database.list_assets()? {
            let inspection = self.inspect(&asset.id)?;
            let content = self.load_revision_content(&inspection.revision)?;
            let files = content
                .files
                .into_iter()
                .map(|file| PortableFile {
                    path: file.relative_path,
                    content_base64: STANDARD.encode(file.bytes),
                    executable: file.executable,
                })
                .collect();
            assets.push(PortableAsset {
                asset: inspection.asset,
                revision: inspection.revision,
                files,
            });
        }
        assets.sort_by(|left, right| left.asset.id.cmp(&right.asset.id));

        let mut assignments = Vec::new();
        for assignment in self.database.list_assignments()? {
            let Some(instance) = self
                .database
                .load_agent_instance(&assignment.agent_instance_id)?
            else {
                continue;
            };
            assignments.push(PortableAssignment {
                asset_id: assignment.asset_id,
                adapter_id: instance.adapter_id,
                profile: instance.profile,
                scope: assignment.scope,
                enabled: assignment.enabled,
            });
        }
        assignments.sort_by(|left, right| {
            (&left.asset_id, &left.adapter_id, &left.profile, &left.scope).cmp(&(
                &right.asset_id,
                &right.adapter_id,
                &right.profile,
                &right.scope,
            ))
        });
        let bundle = PortableBundle {
            schema_version: 1,
            created_at_ms: now_ms(),
            assets,
            assignments,
        };
        let bytes = serde_json::to_vec_pretty(&bundle)?;
        if bytes.len() as u64 > MAX_BUNDLE_BYTES {
            return Err(ServiceError::InvalidInput(
                "导出包超过 64 MiB 限制".to_owned(),
            ));
        }
        atomic_write(output, &bytes)?;
        Ok(bundle)
    }

    /// 导入并逐项验证配置包；不会自动应用到 Agent 文件。
    pub fn import_bundle(&self, input: &Utf8Path) -> ServiceResult<BundleImportReport> {
        let metadata = fs::symlink_metadata(input)
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ServiceError::InvalidInput(
                "导入包必须是真实普通文件".to_owned(),
            ));
        }
        if metadata.len() > MAX_BUNDLE_BYTES {
            return Err(ServiceError::InvalidInput(
                "导入包超过 64 MiB 限制".to_owned(),
            ));
        }
        let bundle: PortableBundle = serde_json::from_slice(
            &fs::read(input).map_err(|error| ServiceError::InvalidInput(error.to_string()))?,
        )?;
        if bundle.schema_version != 1 {
            return Err(ServiceError::InvalidInput(format!(
                "不支持的导入 schema_version：{}",
                bundle.schema_version
            )));
        }

        let mut asset_ids = Vec::new();
        for portable in bundle.assets {
            if portable.asset.current_revision_id.as_deref() != Some(&portable.revision.id) {
                return Err(ServiceError::InvalidInput(format!(
                    "资产 {} 的 current_revision_id 不匹配",
                    portable.asset.id
                )));
            }
            let files = portable
                .files
                .into_iter()
                .map(|file| {
                    Ok(AssetFileContent {
                        relative_path: file.path,
                        bytes: STANDARD.decode(file.content_base64).map_err(|error| {
                            ServiceError::InvalidInput(format!("base64 无效：{error}"))
                        })?,
                        executable: file.executable,
                    })
                })
                .collect::<ServiceResult<Vec<_>>>()?;
            let content = validate_asset_content(AssetContent { files })?;
            verify_revision_content(&portable.revision, &content)?;
            persist_portable(self, &portable.asset, &portable.revision, content)?;
            asset_ids.push(portable.asset.id);
        }
        asset_ids.sort();
        asset_ids.dedup();

        // 分配意图跨机器可能没有对应实例。唯一匹配时只保存为禁用意图，仍需用户
        // 运行 assign 生成并确认计划；其余明确返回 pending，绝不猜 profile。
        let instances = self.database.list_agent_instances()?;
        let mut pending_assignments = Vec::new();
        for portable in bundle.assignments {
            let matches: Vec<_> = instances
                .iter()
                .filter(|instance| {
                    instance.adapter_id == portable.adapter_id
                        && instance.profile == portable.profile
                })
                .collect();
            if matches.len() == 1 && asset_ids.contains(&portable.asset_id) {
                let inspection = self.inspect(&portable.asset_id)?;
                let assignment = Assignment {
                    id: ContentHash::from_bytes(
                        format!(
                            "assignment\0{}\0{}\0{}",
                            portable.asset_id, matches[0].id, portable.scope
                        )
                        .as_bytes(),
                    )
                    .to_string(),
                    asset_id: portable.asset_id,
                    revision_id: inspection.revision.id,
                    agent_instance_id: matches[0].id.clone(),
                    scope: portable.scope,
                    // 导入不等于部署成功，所以始终禁用。
                    enabled: false,
                };
                self.database.save_assignment(&assignment, now_ms())?;
            } else {
                pending_assignments.push(portable);
            }
        }
        Ok(BundleImportReport {
            asset_ids,
            pending_assignments,
        })
    }
}

fn verify_revision_content(revision: &AssetRevision, content: &AssetContent) -> ServiceResult<()> {
    let hashes_match = match &revision.spec {
        AssetSpec::Skill(_) => {
            content.raw_hash()? == revision.raw_hash
                && content.normalized_hash()? == revision.normalized_hash
        }
        AssetSpec::Prompt(_) | AssetSpec::McpServer(_) => {
            let [file] = content.files.as_slice() else {
                return Err(ServiceError::InvalidInput(
                    "Prompt/MCP 修订必须恰好包含一个内容文件".to_owned(),
                ));
            };
            ContentHash::from_bytes(&file.bytes) == revision.raw_hash
                && rigdeck_core::normalized_hash(&file.bytes) == revision.normalized_hash
        }
    };
    if !hashes_match {
        return Err(ServiceError::InvalidInput(format!(
            "修订 {} 的 bundle hash 不匹配",
            revision.id
        )));
    }
    Ok(())
}

fn persist_portable(
    service: &RigDeckService,
    asset: &Asset,
    revision: &AssetRevision,
    content: AssetContent,
) -> ServiceResult<()> {
    match &revision.spec {
        AssetSpec::Skill(skill) => {
            let inventory_bytes = content.inventory_bytes()?;
            if ContentHash::from_bytes(&inventory_bytes) != skill.inventory_object {
                return Err(ServiceError::InvalidInput(
                    "Skill inventory hash 不匹配".to_owned(),
                ));
            }
            persist_imported_skill(
                &ImportedSkill {
                    asset: asset.clone(),
                    revision: revision.clone(),
                    content,
                    inventory_bytes,
                },
                &service.objects,
            )?;
        }
        AssetSpec::Prompt(_) | AssetSpec::McpServer(_) => {
            let [file] = content.files.as_slice() else {
                return Err(ServiceError::InvalidInput(
                    "Prompt/MCP 修订必须恰好包含一个内容文件".to_owned(),
                ));
            };
            let hash = service.objects.put_bytes(&file.bytes)?;
            if hash != revision.content_object {
                return Err(ServiceError::InvalidInput(
                    "Prompt/MCP content_object hash 不匹配".to_owned(),
                ));
            }
        }
    }
    service.database.save_revision(revision)?;
    service.database.save_asset(asset, now_ms())?;
    Ok(())
}

fn atomic_write(path: &Utf8Path, bytes: &[u8]) -> ServiceResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ServiceError::InvalidInput("导出路径没有父目录".to_owned()))?;
    fs::create_dir_all(parent).map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    temporary
        .write_all(bytes)
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    temporary
        .persist(path)
        .map_err(|error| ServiceError::InvalidInput(error.error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use camino::Utf8PathBuf;
    use rigdeck_security::InMemorySecretVault;
    use rigdeck_store::ObjectKey;

    use super::*;
    use crate::AppPaths;

    #[test]
    fn bundle_round_trip_preserves_multifile_skill_without_secret_values() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join("source-data")).unwrap();
        let source = RigDeckService::open_with_key(
            AppPaths::for_root(root),
            ObjectKey::from_bytes([3; 32]),
            Arc::new(InMemorySecretVault::default()),
        )
        .unwrap();
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("skill")).unwrap();
        fs::create_dir_all(skill.join("scripts")).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: portable\n---\n# Portable\n",
        )
        .unwrap();
        fs::write(skill.join("scripts/run.sh"), b"echo portable\n").unwrap();
        let imported = source.add_local_skill(&skill).unwrap();
        let output = Utf8PathBuf::from_path_buf(temp.path().join("bundle.json")).unwrap();
        source.export_bundle(&output).unwrap();
        let bytes = fs::read(&output).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("Bearer "));

        let target = RigDeckService::open_with_key(
            AppPaths::for_root(
                Utf8PathBuf::from_path_buf(temp.path().join("target-data")).unwrap(),
            ),
            ObjectKey::from_bytes([4; 32]),
            Arc::new(InMemorySecretVault::default()),
        )
        .unwrap();
        let report = target.import_bundle(&output).unwrap();
        assert_eq!(report.asset_ids, vec![imported.asset.id.clone()]);
        assert_eq!(target.inspect(&imported.asset.id).unwrap(), imported);
    }
}
