//! Adapter 开发者工具包：脚手架、验证、契约夹具与确定性打包。

use std::{collections::BTreeSet, fs, io::Write};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use rigdeck_core::{AssetKind, ContentHash, SurfaceMode};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    AdapterError, AdapterErrorCode, AdapterManifest, AdapterPackage, AdapterResult,
    AssetCapability, CapabilityOperation, CodecDescriptor, DetectionContext, DetectionRule,
    NativeFormat, Platform, ProtocolRange, ScopeDescriptor, SurfaceDescriptor,
};

const MAX_PACKAGE_FILES: usize = 1_000;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 32;

/// Adapter 包验证报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterValidationReport {
    /// Adapter ID。
    pub adapter_id: String,
    /// 包版本。
    pub version: String,
    /// 文件数量。
    pub file_count: usize,
    /// 总字节数。
    pub total_bytes: u64,
    /// 所有文件内容和相对路径共同决定的 hash。
    pub package_hash: ContentHash,
}

/// Windows/macOS 契约夹具检查报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterContractReport {
    /// Adapter ID。
    pub adapter_id: String,
    /// 已验证平台夹具名称。
    pub fixtures: Vec<String>,
    /// 声明式检测是否能在当前平台夹具中运行。
    pub current_platform_detection_checked: bool,
}

/// 确定性 Adapter bundle。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterBundle {
    /// Bundle schema 版本。
    pub schema_version: u32,
    /// Adapter ID。
    pub adapter_id: String,
    /// Adapter 版本。
    pub version: String,
    /// 包语义 hash。
    pub package_hash: ContentHash,
    /// 相对路径排序后的文件。
    pub files: Vec<AdapterBundleFile>,
}

/// Bundle 中的单个文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterBundleFile {
    /// 安全相对路径。
    pub path: Utf8PathBuf,
    /// 文件 hash。
    pub hash: ContentHash,
    /// Base64 文件内容。
    pub content_base64: String,
}

/// 创建一个最小但可立即验证的 Adapter 工程。
pub fn scaffold_adapter(root: &Utf8Path, adapter_id: &str) -> AdapterResult<AdapterManifest> {
    validate_adapter_id(adapter_id)?;
    if root.exists() {
        let mut entries = fs::read_dir(root).map_err(|error| io_error(root, error))?;
        if entries.next().is_some() {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("脚手架目标必须不存在或为空：{root}"),
            ));
        }
    }
    fs::create_dir_all(root).map_err(|error| io_error(root, error))?;
    let manifest = scaffold_manifest(adapter_id);
    manifest.validate()?;
    write_new(
        &root.join("adapter.json"),
        &serde_json::to_vec_pretty(&manifest).map_err(json_error)?,
    )?;
    write_new(
        &root.join("README.md"),
        format!(
            "# {adapter_id} Adapter\n\n这是 RigDeck Adapter Protocol v1 脚手架。Adapter 只返回投影，禁止直接写 Agent 文件。\n"
        )
        .as_bytes(),
    )?;
    for platform in ["windows", "macos"] {
        let home = root.join(format!("fixtures/{platform}/home/.{adapter_id}"));
        let project = root.join(format!("fixtures/{platform}/project"));
        fs::create_dir_all(&home).map_err(|error| io_error(&home, error))?;
        fs::create_dir_all(&project).map_err(|error| io_error(&project, error))?;
        write_new(&home.join("installed.marker"), b"fixture\n")?;
        write_new(&project.join("README.md"), b"fixture project\n")?;
    }
    Ok(manifest)
}

/// 验证 manifest、路径、symlink 策略和包大小限制。
pub fn validate_adapter_package(root: &Utf8Path) -> AdapterResult<AdapterValidationReport> {
    let package = AdapterPackage::load(root.to_owned())?;
    let files = collect_package_files(root)?;
    let total_bytes = files.iter().map(|file| file.bytes.len() as u64).sum();
    let package_hash = package_hash(&files);
    Ok(AdapterValidationReport {
        adapter_id: package.manifest().adapter_id.clone(),
        version: package.manifest().version.clone(),
        file_count: files.len(),
        total_bytes,
        package_hash,
    })
}

/// 检查 Windows/macOS 黄金夹具目录，并在当前平台运行声明式检测。
pub fn test_adapter_package(root: &Utf8Path) -> AdapterResult<AdapterContractReport> {
    let package = AdapterPackage::load(root.to_owned())?;
    let mut fixtures = Vec::new();
    for platform in ["windows", "macos"] {
        let fixture = root.join(format!("fixtures/{platform}"));
        let home = fixture.join("home");
        let project = fixture.join("project");
        if !home.is_dir() || !project.is_dir() {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                format!("缺少 {platform} fixture 的 home/project：{fixture}"),
            ));
        }
        validate_fixture_tree(&fixture)?;
        fixtures.push(platform.to_owned());
    }
    let current = match Platform::current() {
        Platform::Windows => "windows",
        Platform::Macos => "macos",
        Platform::Linux => "windows",
    };
    let context = DetectionContext {
        home: root.join(format!("fixtures/{current}/home")),
        project_root: Some(root.join(format!("fixtures/{current}/project"))),
    };
    // Linux 只承担可移植契约开发，不声称是 GA 平台；manifest 通常仍包含 Linux，
    // 以便 WSL CI 执行相同检测逻辑。
    let detected = package.detect(&context)?;
    if detected.is_empty() {
        return Err(AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            "当前平台 fixture 没有触发任何 detection rule",
        ));
    }
    Ok(AdapterContractReport {
        adapter_id: package.manifest().adapter_id.clone(),
        fixtures,
        current_platform_detection_checked: true,
    })
}

/// 创建内容确定的 `.rigdeck-adapter` JSON bundle。
pub fn pack_adapter(root: &Utf8Path, output: &Utf8Path) -> AdapterResult<AdapterBundle> {
    let validation = validate_adapter_package(root)?;
    let files = collect_package_files(root)?;
    let bundle = AdapterBundle {
        schema_version: 1,
        adapter_id: validation.adapter_id,
        version: validation.version,
        package_hash: validation.package_hash,
        files: files
            .into_iter()
            .map(|file| AdapterBundleFile {
                path: file.path,
                hash: ContentHash::from_bytes(&file.bytes),
                content_base64: BASE64.encode(file.bytes),
            })
            .collect(),
    };
    validate_bundle(&bundle)?;
    let bytes = serde_json::to_vec_pretty(&bundle).map_err(json_error)?;
    atomic_write(output, &bytes)?;
    Ok(bundle)
}

/// 验证 bundle 中的路径、Base64、单文件 hash 与整体 hash；不会解包或写文件。
pub fn validate_bundle(bundle: &AdapterBundle) -> AdapterResult<()> {
    if bundle.schema_version != 1 || bundle.files.is_empty() {
        return Err(AdapterError::new(
            AdapterErrorCode::InvalidManifest,
            "Adapter bundle schema 或文件列表无效",
        ));
    }
    if bundle.files.len() > MAX_PACKAGE_FILES {
        return Err(AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            "Adapter bundle 文件数超过限制",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut files = Vec::new();
    let mut total = 0u64;
    for file in &bundle.files {
        validate_relative(&file.path)?;
        if !seen.insert(file.path.clone()) {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("Bundle 路径重复：{}", file.path),
            ));
        }
        let bytes = BASE64.decode(&file.content_base64).map_err(|error| {
            AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                format!("Bundle Base64 无效（{}）：{error}", file.path),
            )
        })?;
        if bytes.len() as u64 > MAX_FILE_BYTES || ContentHash::from_bytes(&bytes) != file.hash {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                format!("Bundle 文件大小或 hash 无效：{}", file.path),
            ));
        }
        total = total.saturating_add(bytes.len() as u64);
        files.push(PackageFile {
            path: file.path.clone(),
            bytes,
        });
    }
    if total > MAX_PACKAGE_BYTES || package_hash(&files) != bundle.package_hash {
        return Err(AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            "Bundle 总大小或 package hash 无效",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct PackageFile {
    path: Utf8PathBuf,
    bytes: Vec<u8>,
}

fn collect_package_files(root: &Utf8Path) -> AdapterResult<Vec<PackageFile>> {
    let metadata = fs::symlink_metadata(root).map_err(|error| io_error(root, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "Adapter 包根必须是真实目录",
        ));
    }
    let mut pending = vec![(root.to_owned(), 0usize)];
    let mut files = Vec::new();
    let mut total = 0u64;
    while let Some((path, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                "Adapter 包目录深度超过限制",
            ));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("Adapter 包禁止 symlink：{path}"),
            ));
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&path).map_err(|error| io_error(&path, error))? {
                let entry = entry.map_err(|error| io_error(&path, error))?;
                let child = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                    AdapterError::new(
                        AdapterErrorCode::PathViolation,
                        format!("Adapter 包路径不是 UTF-8：{}", path.display()),
                    )
                })?;
                pending.push((child, depth + 1));
            }
        } else if metadata.is_file() {
            if metadata.len() > MAX_FILE_BYTES {
                return Err(AdapterError::new(
                    AdapterErrorCode::ValidationFailed,
                    format!("Adapter 单文件超过 16 MiB：{path}"),
                ));
            }
            let relative = path.strip_prefix(root).map_err(|_| {
                AdapterError::new(AdapterErrorCode::PathViolation, "包文件逃逸根目录")
            })?;
            validate_relative(relative)?;
            let bytes = fs::read(&path).map_err(|error| io_error(&path, error))?;
            total = total.saturating_add(bytes.len() as u64);
            if total > MAX_PACKAGE_BYTES || files.len() >= MAX_PACKAGE_FILES {
                return Err(AdapterError::new(
                    AdapterErrorCode::ValidationFailed,
                    "Adapter 包总大小或文件数超过限制",
                ));
            }
            files.push(PackageFile {
                path: relative.to_owned(),
                bytes,
            });
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn validate_fixture_tree(root: &Utf8Path) -> AdapterResult<()> {
    let _ = collect_package_files(root)?;
    Ok(())
}

fn package_hash(files: &[PackageFile]) -> ContentHash {
    let mut material = Vec::new();
    for file in files {
        material.extend_from_slice(file.path.as_str().as_bytes());
        material.push(0);
        material.extend_from_slice(ContentHash::from_bytes(&file.bytes).as_str().as_bytes());
        material.push(0);
    }
    ContentHash::from_bytes(&material)
}

fn scaffold_manifest(adapter_id: &str) -> AdapterManifest {
    AdapterManifest {
        schema_version: 1,
        adapter_id: adapter_id.to_owned(),
        version: "0.1.0".to_owned(),
        display_name: adapter_id.to_owned(),
        protocol: ProtocolRange { min: 1, max: 1 },
        platforms: BTreeSet::from([Platform::Windows, Platform::Macos, Platform::Linux]),
        detection: vec![DetectionRule {
            path: format!("{{home}}/.{adapter_id}"),
            markers: vec!["installed.marker".to_owned()],
            profile: None,
            version_hint: None,
        }],
        capabilities: vec![AssetCapability {
            asset_kind: AssetKind::Skill,
            scopes: vec!["global".to_owned()],
            operations: BTreeSet::from([
                CapabilityOperation::Import,
                CapabilityOperation::Install,
                CapabilityOperation::Update,
                CapabilityOperation::Remove,
                CapabilityOperation::Drift,
            ]),
        }],
        scopes: vec![ScopeDescriptor {
            id: "global".to_owned(),
            display_name: "全局".to_owned(),
            project_required: false,
        }],
        native_formats: vec![NativeFormat {
            id: "skill-directory".to_owned(),
            preserves_comments: true,
            preserves_unknown_fields: true,
        }],
        codecs: vec![CodecDescriptor {
            id: "agent-skill-v1".to_owned(),
            asset_kind: AssetKind::Skill,
            native_format: "skill-directory".to_owned(),
        }],
        surfaces: vec![SurfaceDescriptor {
            id: "global-skills".to_owned(),
            scope: "global".to_owned(),
            asset_kind: AssetKind::Skill,
            root: "{home}".to_owned(),
            target: format!(".{adapter_id}/skills/{{name}}"),
            native_format: "skill-directory".to_owned(),
            section: None,
            mode: SurfaceMode::DirectoryTree,
            writable: true,
            precedence: 0,
        }],
        limitations: Vec::new(),
        official_docs: vec!["https://example.invalid/replace-with-official-docs".to_owned()],
        helper: None,
        deprecation: None,
    }
}

fn validate_adapter_id(value: &str) -> AdapterResult<()> {
    let valid = !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-._".contains(&byte)
        });
    if !valid {
        return Err(AdapterError::new(
            AdapterErrorCode::InvalidManifest,
            "Adapter ID 只能使用小写 ASCII、数字、点、横线和下划线",
        ));
    }
    Ok(())
}

fn validate_relative(path: &Utf8Path) -> AdapterResult<()> {
    if path.as_str().is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Utf8Component::ParentDir | Utf8Component::RootDir | Utf8Component::Prefix(_)
            )
        })
    {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("Bundle 路径必须是安全相对路径：{path}"),
        ));
    }
    Ok(())
}

fn write_new(path: &Utf8Path, bytes: &[u8]) -> AdapterResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(path).map_err(|error| io_error(path, error))?;
    file.write_all(bytes)
        .map_err(|error| io_error(path, error))?;
    file.sync_all().map_err(|error| io_error(path, error))?;
    Ok(())
}

fn atomic_write(path: &Utf8Path, bytes: &[u8]) -> AdapterResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AdapterError::new(AdapterErrorCode::PathViolation, "输出文件没有父目录"))?;
    fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    let mut temporary = NamedTempFile::new_in(parent).map_err(|error| io_error(parent, error))?;
    temporary
        .write_all(bytes)
        .map_err(|error| io_error(temporary.path(), error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| io_error(temporary.path(), error))?;
    temporary
        .persist(path)
        .map_err(|error| io_error(path, error.error))?;
    Ok(())
}

fn io_error(path: impl AsRef<std::path::Path>, error: std::io::Error) -> AdapterError {
    AdapterError::new(
        AdapterErrorCode::Io,
        format!("文件系统错误（{}）：{error}", path.as_ref().display()),
    )
}

fn json_error(error: serde_json::Error) -> AdapterError {
    AdapterError::new(
        AdapterErrorCode::InvalidManifest,
        format!("JSON 编解码失败：{error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_validate_test_and_pack_are_reproducible() {
        let temp = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let root = base.join("demo-adapter");
        scaffold_adapter(&root, "demo-agent").unwrap();
        let validation = validate_adapter_package(&root).unwrap();
        assert_eq!(validation.adapter_id, "demo-agent");
        let contract = test_adapter_package(&root).unwrap();
        assert_eq!(contract.fixtures, vec!["windows", "macos"]);

        let first = base.join("first.rigdeck-adapter");
        let second = base.join("second.rigdeck-adapter");
        let first_bundle = pack_adapter(&root, &first).unwrap();
        let second_bundle = pack_adapter(&root, &second).unwrap();
        assert_eq!(first_bundle, second_bundle);
        assert_eq!(fs::read(first).unwrap(), fs::read(second).unwrap());
    }

    #[test]
    fn bundle_traversal_and_hash_tampering_fail_closed() {
        let mut bundle = AdapterBundle {
            schema_version: 1,
            adapter_id: "demo".to_owned(),
            version: "1.0.0".to_owned(),
            package_hash: ContentHash::from_bytes(b"wrong"),
            files: vec![AdapterBundleFile {
                path: "../escape".into(),
                hash: ContentHash::from_bytes(b"value"),
                content_base64: BASE64.encode(b"value"),
            }],
        };
        assert!(validate_bundle(&bundle).is_err());
        bundle.files[0].path = "adapter.json".into();
        assert!(validate_bundle(&bundle).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn package_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let root = base.join("adapter");
        scaffold_adapter(&root, "demo-agent").unwrap();
        fs::write(base.join("outside"), b"secret").unwrap();
        symlink(base.join("outside"), root.join("linked")).unwrap();
        let error = validate_adapter_package(&root).unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::PathViolation);
    }
}
