//! 运行时 Adapter 包加载与无脚本检测。

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use rigdeck_core::{AgentHealth, AgentInstance, ContentHash};

use crate::{AdapterError, AdapterErrorCode, AdapterManifest, AdapterResult, DetectionContext};

/// 已验证但尚未执行任何 helper 的 Adapter 包。
#[derive(Debug, Clone)]
pub struct AdapterPackage {
    root: Utf8PathBuf,
    manifest: AdapterManifest,
}

impl AdapterPackage {
    /// 从目录加载 `adapter.json`。
    pub fn load(root: impl Into<Utf8PathBuf>) -> AdapterResult<Self> {
        let root = root.into();
        let manifest_path = root.join("adapter.json");
        let metadata = fs::symlink_metadata(&manifest_path).map_err(|error| {
            AdapterError::new(
                AdapterErrorCode::Io,
                format!("无法读取 {manifest_path}：{error}"),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                "adapter.json 不能是 symlink",
            ));
        }
        let bytes = fs::read(&manifest_path).map_err(|error| {
            AdapterError::new(
                AdapterErrorCode::Io,
                format!("无法读取 {manifest_path}：{error}"),
            )
        })?;
        let manifest: AdapterManifest = serde_json::from_slice(&bytes).map_err(|error| {
            AdapterError::new(
                AdapterErrorCode::InvalidManifest,
                format!("adapter.json JSON 无效：{error}"),
            )
        })?;
        manifest.validate()?;
        Ok(Self { root, manifest })
    }

    /// Adapter 包根目录。
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// 已验证 manifest。
    pub fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    /// 只按声明路径和 marker 检测实例；不会运行 version command/helper。
    pub fn detect(&self, context: &DetectionContext) -> AdapterResult<Vec<AgentInstance>> {
        if !self.manifest.supports_current_platform() {
            return Err(AdapterError::new(
                AdapterErrorCode::UnsupportedPlatform,
                "Adapter 不支持当前平台",
            ));
        }
        let mut instances = Vec::new();
        for rule in &self.manifest.detection {
            let Some(path) = expand_path(&rule.path, context)? else {
                continue;
            };
            if !path.exists() {
                continue;
            }
            let allowed_root = if rule.path.starts_with("{home}") {
                &context.home
            } else {
                context.project_root.as_ref().ok_or_else(|| {
                    AdapterError::new(
                        AdapterErrorCode::PathViolation,
                        "project 检测规则缺少 project root",
                    )
                })?
            };
            let path = canonicalize_inside(&path, allowed_root)?;
            let all_markers_exist = rule.markers.iter().all(|marker| {
                let marker = path.join(marker);
                marker.exists() && canonicalize_inside(&marker, &path).is_ok()
            });
            if !all_markers_exist {
                continue;
            }
            let material = format!(
                "{}\0{}\0{}",
                self.manifest.adapter_id,
                path,
                rule.profile.as_deref().unwrap_or("default")
            );
            instances.push(AgentInstance {
                id: ContentHash::from_bytes(material.as_bytes()).to_string(),
                adapter_id: self.manifest.adapter_id.clone(),
                display_name: self.manifest.display_name.clone(),
                version: None,
                managed_roots: vec![path],
                profile: rule.profile.clone(),
                health: AgentHealth::Healthy,
                surfaces: Vec::new(),
            });
        }
        Ok(instances)
    }

    /// 判断 helper hash 是否有对应的显式信任决定。
    pub fn helper_is_trusted(&self, trusted_hash: Option<&ContentHash>) -> AdapterResult<bool> {
        let Some(helper) = &self.manifest.helper else {
            return Ok(true);
        };
        let declared: ContentHash =
            helper
                .hash
                .parse()
                .map_err(|error: rigdeck_core::CoreError| {
                    AdapterError::new(AdapterErrorCode::InvalidManifest, error.to_string())
                })?;
        Ok(trusted_hash == Some(&declared))
    }
}

fn canonicalize_inside(path: &Utf8Path, root: &Utf8Path) -> AdapterResult<Utf8PathBuf> {
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        AdapterError::new(
            AdapterErrorCode::Io,
            format!("无法 canonicalize 根目录 {root}：{error}"),
        )
    })?;
    let canonical_path = fs::canonicalize(path).map_err(|error| {
        AdapterError::new(
            AdapterErrorCode::Io,
            format!("无法 canonicalize 检测路径 {path}：{error}"),
        )
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("检测路径通过 symlink 逃逸允许根目录：{path}"),
        ));
    }
    Utf8PathBuf::from_path_buf(canonical_path).map_err(|path| {
        AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("检测路径不是 UTF-8：{}", path.display()),
        )
    })
}

fn expand_path(template: &str, context: &DetectionContext) -> AdapterResult<Option<Utf8PathBuf>> {
    let (root, suffix) = if let Some(suffix) = template.strip_prefix("{home}") {
        (&context.home, suffix)
    } else if let Some(suffix) = template.strip_prefix("{project}") {
        let Some(project) = context.project_root.as_ref() else {
            return Ok(None);
        };
        (project, suffix)
    } else {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "检测路径模板缺少允许的根 token",
        ));
    };
    let suffix = suffix.trim_start_matches(['/', '\\']);
    let path = root.join(suffix);
    if path
        .components()
        .any(|part| matches!(part, camino::Utf8Component::ParentDir))
    {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "展开后的检测路径包含 `..`",
        ));
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs};

    use super::*;
    use crate::{
        AssetCapability, CapabilityOperation, DetectionRule, NativeFormat, Platform, ProtocolRange,
        ScopeDescriptor,
    };

    fn manifest_json(path: &str) -> String {
        let manifest = AdapterManifest {
            schema_version: 1,
            adapter_id: "mock-agent".to_owned(),
            version: "1.0.0".to_owned(),
            display_name: "Mock Agent".to_owned(),
            protocol: ProtocolRange { min: 1, max: 1 },
            platforms: BTreeSet::from([Platform::current()]),
            detection: vec![DetectionRule {
                path: path.to_owned(),
                markers: vec!["installed.marker".to_owned()],
                profile: None,
                version_hint: None,
            }],
            capabilities: vec![AssetCapability {
                asset_kind: rigdeck_core::AssetKind::Skill,
                scopes: vec!["global".to_owned()],
                operations: BTreeSet::from([CapabilityOperation::Install]),
            }],
            scopes: vec![ScopeDescriptor {
                id: "global".to_owned(),
                display_name: "全局".to_owned(),
                project_required: false,
            }],
            native_formats: vec![NativeFormat {
                id: "directory".to_owned(),
                preserves_comments: true,
                preserves_unknown_fields: true,
            }],
            codecs: Vec::new(),
            surfaces: Vec::new(),
            limitations: Vec::new(),
            official_docs: vec!["https://example.invalid/mock".to_owned()],
            helper: None,
            deprecation: None,
        };
        serde_json::to_string_pretty(&manifest).unwrap()
    }

    #[test]
    fn mock_adapter_package_detects_without_core_change() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let package_root = root.join("adapter");
        let agent_root = root.join(".mock-agent");
        fs::create_dir_all(&package_root).unwrap();
        fs::create_dir_all(&agent_root).unwrap();
        fs::write(agent_root.join("installed.marker"), b"ok").unwrap();
        fs::write(
            package_root.join("adapter.json"),
            manifest_json("{home}/.mock-agent"),
        )
        .unwrap();

        let package = AdapterPackage::load(package_root).unwrap();
        let instances = package
            .detect(&DetectionContext {
                home: root,
                project_root: None,
            })
            .unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].adapter_id, "mock-agent");
    }

    #[test]
    fn traversal_in_manifest_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        fs::write(root.join("adapter.json"), manifest_json("{home}/../escape")).unwrap();
        let error = AdapterPackage::load(root).unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::PathViolation);
    }

    #[test]
    fn incompatible_protocol_fails_closed() {
        let mut value: serde_json::Value =
            serde_json::from_str(&manifest_json("{home}/.mock-agent")).unwrap();
        value["protocol"]["min"] = 2.into();
        value["protocol"]["max"] = 2.into();
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        fs::write(
            root.join("adapter.json"),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        let error = AdapterPackage::load(root).unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::IncompatibleProtocol);
    }

    #[test]
    fn malformed_manifest_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        fs::write(root.join("adapter.json"), b"{ not-json }").unwrap();
        let error = AdapterPackage::load(root).unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::InvalidManifest);
    }

    #[cfg(unix)]
    #[test]
    fn detection_symlink_escape_fails_closed() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join("home")).unwrap();
        let outside = Utf8PathBuf::from_path_buf(temp.path().join("outside")).unwrap();
        let package_root = Utf8PathBuf::from_path_buf(temp.path().join("adapter")).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&package_root).unwrap();
        fs::write(outside.join("installed.marker"), b"ok").unwrap();
        symlink(&outside, root.join(".mock-agent")).unwrap();
        fs::write(
            package_root.join("adapter.json"),
            manifest_json("{home}/.mock-agent"),
        )
        .unwrap();

        let package = AdapterPackage::load(package_root).unwrap();
        let error = package
            .detect(&DetectionContext {
                home: root,
                project_root: None,
            })
            .unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::PathViolation);
    }
}
