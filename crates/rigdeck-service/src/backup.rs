//! 应用数据库与加密对象库的一致性备份/恢复。

use std::{collections::BTreeSet, fs, io::Write};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use rigdeck_core::{AuditEvent, ContentHash};
use rigdeck_store::Database;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{now_ms, RigDeckService, ServiceError, ServiceResult};

/// 备份中的一个加密文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupFile {
    /// 相对备份根路径。
    pub path: Utf8PathBuf,
    /// 备份字节 hash（对象文件仍是密文）。
    pub hash: ContentHash,
    /// 文件长度。
    pub size: u64,
}

/// 可验证备份 manifest。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Schema 版本。
    pub schema_version: u32,
    /// 备份 ID。
    pub id: String,
    /// 创建时间。
    pub created_at_ms: i64,
    /// SQLite 一致性备份文件。
    pub database: BackupFile,
    /// 加密对象文件清单。
    pub objects: Vec<BackupFile>,
}

impl RigDeckService {
    /// 创建数据库和全部加密对象的一致性备份。
    pub fn create_backup(&self) -> ServiceResult<BackupManifest> {
        fs::create_dir_all(&self.paths.backups)
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        let created_at_ms = now_ms();
        let staging = self.paths.backups.join(format!(".pending-{created_at_ms}"));
        fs::create_dir(&staging).map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        let database_path = staging.join("rigdeck.sqlite3");
        self.database.backup(&database_path)?;
        let database = backup_file(&staging, &database_path)?;
        let source_objects = self.paths.object_store.join("objects");
        let destination_objects = staging.join("objects");
        copy_tree(&source_objects, &destination_objects)?;
        let mut objects = collect_files(&staging, &destination_objects)?;
        objects.sort_by(|left, right| left.path.cmp(&right.path));
        let mut identity = Vec::new();
        identity.extend_from_slice(database.hash.as_str().as_bytes());
        for object in &objects {
            identity.extend_from_slice(object.path.as_str().as_bytes());
            identity.push(0);
            identity.extend_from_slice(object.hash.as_str().as_bytes());
        }
        identity.extend_from_slice(created_at_ms.to_string().as_bytes());
        let id = ContentHash::from_bytes(&identity).to_string();
        let manifest = BackupManifest {
            schema_version: 1,
            id: id.clone(),
            created_at_ms,
            database,
            objects,
        };
        let manifest_path = staging.join("manifest.json");
        write_synced(&manifest_path, &serde_json::to_vec_pretty(&manifest)?)?;
        let destination = self.paths.backups.join(&id);
        fs::rename(&staging, &destination)
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        Ok(manifest)
    }

    /// 列出并验证 manifest 基本结构；完整字节校验在恢复前执行。
    pub fn list_backups(&self) -> ServiceResult<Vec<BackupManifest>> {
        if !self.paths.backups.exists() {
            return Ok(Vec::new());
        }
        let mut manifests = Vec::new();
        for entry in fs::read_dir(&self.paths.backups)
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?
        {
            let entry = entry.map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
            if !entry
                .file_type()
                .map_err(|error| ServiceError::InvalidInput(error.to_string()))?
                .is_dir()
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !valid_backup_id(&name) {
                continue;
            }
            let path = Utf8PathBuf::from_path_buf(entry.path())
                .map_err(|path| ServiceError::InvalidInput(path.display().to_string()))?;
            let manifest: BackupManifest = serde_json::from_slice(
                &fs::read(path.join("manifest.json"))
                    .map_err(|error| ServiceError::InvalidInput(error.to_string()))?,
            )?;
            if manifest.id != name || manifest.schema_version != 1 {
                return Err(ServiceError::InvalidInput(format!(
                    "备份 manifest ID/schema 无效：{name}"
                )));
            }
            manifests.push(manifest);
        }
        manifests.sort_by_key(|right| std::cmp::Reverse(right.created_at_ms));
        Ok(manifests)
    }

    /// 完整校验指定备份的 manifest、路径、长度和内容 hash，但不执行恢复。
    pub fn verify_backup(&self, id: &str) -> ServiceResult<BackupManifest> {
        if !valid_backup_id(id) {
            return Err(ServiceError::InvalidInput("备份 ID 无效".to_owned()));
        }
        let root = self.paths.backups.join(id);
        let manifest: BackupManifest = serde_json::from_slice(
            &fs::read(root.join("manifest.json"))
                .map_err(|error| ServiceError::InvalidInput(error.to_string()))?,
        )?;
        if manifest.id != id {
            return Err(ServiceError::InvalidInput(
                "备份目录与 manifest ID 不一致".to_owned(),
            ));
        }
        verify_manifest(&root, &manifest)?;
        Ok(manifest)
    }

    /// 恢复备份。恢复前自动创建 recovery 备份；失败时恢复数据库 recovery 副本。
    pub fn restore_backup(&mut self, id: &str) -> ServiceResult<BackupManifest> {
        let manifest = self.verify_backup(id)?;
        let root = self.paths.backups.join(id);
        let recovery = self.create_backup()?;
        let recovery_db = self
            .paths
            .backups
            .join(&recovery.id)
            .join(&recovery.database.path);

        for object in &manifest.objects {
            let source = root.join(&object.path);
            let relative = object.path.strip_prefix("objects").map_err(|_| {
                ServiceError::InvalidInput("备份 object 路径不在 objects/ 下".to_owned())
            })?;
            let destination = self.paths.object_store.join("objects").join(relative);
            copy_file_noclobber(&source, &destination)?;
        }
        self.objects.integrity_check()?;
        let database_source = root.join(&manifest.database.path);
        if let Err(error) = self.database.restore(&database_source) {
            let _ = self.database.restore(&recovery_db);
            return Err(error.into());
        }
        let event = AuditEvent {
            id: ContentHash::from_bytes(format!("audit\0restore\0{id}\0{}", now_ms()).as_bytes())
                .to_string(),
            event_type: "backup_restored".to_owned(),
            plan_id: None,
            details: serde_json::json!({
                "backup_id": id,
                "recovery_backup_id": recovery.id,
            }),
            created_at_ms: now_ms(),
        };
        self.database.save_audit_event(&event)?;
        Ok(manifest)
    }

    /// 为“冲突内恢复”准备只读备份数据库，并把缺失的加密对象安全暂存回当前对象库。
    /// 该方法不恢复主数据库，也不触碰 Agent 文件；真正变更仍由 Planner 执行。
    pub(crate) fn open_backup_for_plan(&self, id: &str) -> ServiceResult<Database> {
        let manifest = self.verify_backup(id)?;
        let root = self.paths.backups.join(id);
        for object in &manifest.objects {
            let relative = object.path.strip_prefix("objects").map_err(|_| {
                ServiceError::InvalidInput("备份 object 路径不在 objects/ 下".to_owned())
            })?;
            copy_file_noclobber(
                &root.join(&object.path),
                &self.paths.object_store.join("objects").join(relative),
            )?;
        }
        self.objects.integrity_check()?;
        Ok(Database::open_read_only(
            root.join(&manifest.database.path),
        )?)
    }
}

fn verify_manifest(root: &Utf8Path, manifest: &BackupManifest) -> ServiceResult<()> {
    if manifest.schema_version != 1 || !valid_backup_id(&manifest.id) {
        return Err(ServiceError::InvalidInput(
            "备份 manifest schema/ID 无效".to_owned(),
        ));
    }
    let mut paths = BTreeSet::new();
    for file in std::iter::once(&manifest.database).chain(&manifest.objects) {
        validate_relative(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(ServiceError::InvalidInput(format!(
                "备份路径重复：{}",
                file.path
            )));
        }
        let bytes = fs::read(root.join(&file.path))
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        if bytes.len() as u64 != file.size || ContentHash::from_bytes(&bytes) != file.hash {
            return Err(ServiceError::InvalidInput(format!(
                "备份文件大小/hash 失败：{}",
                file.path
            )));
        }
    }
    Ok(())
}

fn copy_tree(source: &Utf8Path, destination: &Utf8Path) -> ServiceResult<()> {
    fs::create_dir_all(destination)
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    for entry in
        fs::read_dir(source).map_err(|error| ServiceError::InvalidInput(error.to_string()))?
    {
        let entry = entry.map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        let metadata = entry
            .file_type()
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        if metadata.is_symlink() {
            return Err(ServiceError::InvalidInput(
                "对象库备份拒绝 symlink".to_owned(),
            ));
        }
        let source_path = Utf8PathBuf::from_path_buf(entry.path())
            .map_err(|path| ServiceError::InvalidInput(path.display().to_string()))?;
        let target = destination.join(entry.file_name().to_string_lossy().as_ref());
        if metadata.is_dir() {
            copy_tree(&source_path, &target)?;
        } else if metadata.is_file() {
            copy_file_noclobber(&source_path, &target)?;
        }
    }
    Ok(())
}

fn copy_file_noclobber(source: &Utf8Path, destination: &Utf8Path) -> ServiceResult<()> {
    let bytes = fs::read(source).map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    if destination.exists() {
        let existing =
            fs::read(destination).map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        if existing != bytes {
            return Err(ServiceError::InvalidInput(format!(
                "不可变备份目标已存在但内容不同：{destination}"
            )));
        }
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    }
    let mut temporary = NamedTempFile::new_in(
        destination
            .parent()
            .ok_or_else(|| ServiceError::InvalidInput("目标没有父目录".to_owned()))?,
    )
    .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    temporary
        .write_all(&bytes)
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    temporary
        .persist_noclobber(destination)
        .map_err(|error| ServiceError::InvalidInput(error.error.to_string()))?;
    Ok(())
}

fn collect_files(root: &Utf8Path, current: &Utf8Path) -> ServiceResult<Vec<BackupFile>> {
    let mut output = Vec::new();
    for entry in
        fs::read_dir(current).map_err(|error| ServiceError::InvalidInput(error.to_string()))?
    {
        let entry = entry.map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        let kind = entry
            .file_type()
            .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
        if kind.is_symlink() {
            return Err(ServiceError::InvalidInput("备份中禁止 symlink".to_owned()));
        }
        let path = Utf8PathBuf::from_path_buf(entry.path())
            .map_err(|path| ServiceError::InvalidInput(path.display().to_string()))?;
        if kind.is_dir() {
            output.extend(collect_files(root, &path)?);
        } else if kind.is_file() {
            output.push(backup_file(root, &path)?);
        }
    }
    Ok(output)
}

fn backup_file(root: &Utf8Path, path: &Utf8Path) -> ServiceResult<BackupFile> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| ServiceError::InvalidInput("备份文件逃逸根目录".to_owned()))?
        .to_owned();
    validate_relative(&relative)?;
    let bytes = fs::read(path).map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    Ok(BackupFile {
        path: relative,
        hash: ContentHash::from_bytes(&bytes),
        size: bytes.len() as u64,
    })
}

fn validate_relative(path: &Utf8Path) -> ServiceResult<()> {
    if path.as_str().is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Utf8Component::ParentDir | Utf8Component::RootDir | Utf8Component::Prefix(_)
            )
        })
    {
        return Err(ServiceError::InvalidInput(format!("备份路径无效：{path}")));
    }
    Ok(())
}

fn valid_backup_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn write_synced(path: &Utf8Path, bytes: &[u8]) -> ServiceResult<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    file.write_all(bytes)
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    file.sync_all()
        .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rigdeck_security::InMemorySecretVault;
    use rigdeck_store::ObjectKey;

    use super::*;
    use crate::AppPaths;

    #[test]
    fn backup_and_restore_recover_database_and_objects() {
        let temp = tempfile::tempdir().unwrap();
        let data = Utf8PathBuf::from_path_buf(temp.path().join("data")).unwrap();
        let mut service = RigDeckService::open_with_key(
            AppPaths::for_root(data),
            ObjectKey::from_bytes([9; 32]),
            Arc::new(InMemorySecretVault::default()),
        )
        .unwrap();
        let skill = Utf8PathBuf::from_path_buf(temp.path().join("skill")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            b"---\nname: backup-demo\n---\n# Demo\n",
        )
        .unwrap();
        let imported = service.add_local_skill(&skill).unwrap();
        let backup = service.create_backup().unwrap();
        assert_eq!(service.list_backups().unwrap()[0].id, backup.id);

        let second = Utf8PathBuf::from_path_buf(temp.path().join("second")).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(
            second.join("SKILL.md"),
            b"---\nname: second\n---\n# Second\n",
        )
        .unwrap();
        service.add_local_skill(&second).unwrap();
        assert_eq!(service.status().unwrap().asset_count, 2);
        service.restore_backup(&backup.id).unwrap();
        assert_eq!(service.status().unwrap().asset_count, 1);
        assert_eq!(service.inspect(&imported.asset.id).unwrap(), imported);

        let backup_root = service.paths.backups.join(&backup.id);
        fs::write(backup_root.join(&backup.database.path), b"tampered").unwrap();
        assert!(service.verify_backup(&backup.id).is_err());
    }
}
