//! 七个内置 manifest 的声明式执行器。

use std::{collections::BTreeSet, fs};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use rigdeck_adapter_sdk::{
    AdapterError, AdapterErrorCode, AdapterHealthReport, AdapterManifest, AdapterPlan,
    AdapterResult, AgentAdapter, AssetContent, AssetValidation, DetectionContext, RemovalIntent,
    RemovalStrategy, RenderOutput, RenderedObject, ScannedEntry, VerificationReport,
};
use rigdeck_core::{
    normalized_hash, verify_hashes, AgentHealth, AgentInstance, Asset, AssetKind, AssetRevision,
    AssetSpec, BindingValue, CompatibilityLoss, ContentHash, McpTransport, NativeSurface,
    ObservationIssue, ProjectedFile, Projection, ProjectionStrategy, SecretRef, SurfaceMode,
};

use crate::{
    classify_managed_document, contains_managed_marker, encode_mcp_entry, inspect_managed_block,
    validate_managed_document, ManagedBlockIssue, ManagedBlockState,
};

/// 按 adapter_id 查 CLI 版本命令，执行后提取版本号。
/// 命令不存在或执行失败时返回 None，不影响实例检测。
fn detect_cli_version(adapter_id: &str) -> Option<String> {
    let (program, args) = match adapter_id {
        "claude-code" => ("claude", vec!["--version"]),
        "codex" => ("codex", vec!["--version"]),
        "opencode" => ("opencode", vec!["--version"]),
        "hermes" => ("hermes", vec!["--version"]),
        "devin" => ("devin", vec!["--version"]),
        "pi" => ("pi", vec!["--version"]),
        "antigravity" => return None, // VS Code 扩展，无 CLI
        _ => return None,
    };
    // Windows 上 npm 全局安装的 CLI 是 .cmd 脚本，CreateProcess 不会自动查找
    // .cmd 扩展名，需要用 cmd /C 包装。.exe 文件也能通过 cmd /C 正常执行。
    // 必须设置 CREATE_NO_WINDOW 标志，否则每次调用都会弹一个 cmd 黑框。
    #[cfg(windows)]
    let output = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut cmd = std::process::Command::new("cmd");
        cmd.creation_flags(CREATE_NO_WINDOW)
            .arg("/C")
            .arg(program)
            .args(&args);
        cmd.output().ok()?
    };
    #[cfg(not(windows))]
    let output = {
        std::process::Command::new(program)
            .args(&args)
            .output()
            .ok()?
    };
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    extract_version_number(&text)
}

/// 从命令输出中提取第一个 x.y.z 格式的版本号。
fn extract_version_number(text: &str) -> Option<String> {
    let mut chars = text.chars().peekable();
    let mut current = String::new();
    let mut found_digit = false;
    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() {
            found_digit = true;
            current.push(ch);
        } else if ch == '.' && found_digit {
            current.push(ch);
        } else if found_digit && !ch.is_ascii_digit() && ch != '.' {
            if current.ends_with('.') {
                current.pop();
            }
            if current.contains('.') {
                return Some(current);
            }
            current.clear();
            found_digit = false;
        }
    }
    if current.contains('.') && !current.ends_with('.') {
        Some(current)
    } else {
        None
    }
}

/// 首发 Adapter 的稳定 ID，顺序也用于 CLI 展示。
pub const BUILTIN_ADAPTER_IDS: [&str; 7] = [
    "claude-code",
    "codex",
    "opencode",
    "hermes",
    "antigravity",
    "pi",
    "devin",
];

const MANIFESTS: [&str; 7] = [
    include_str!("../manifests/claude-code.json"),
    include_str!("../manifests/codex.json"),
    include_str!("../manifests/opencode.json"),
    include_str!("../manifests/hermes.json"),
    include_str!("../manifests/antigravity.json"),
    include_str!("../manifests/pi.json"),
    include_str!("../manifests/devin.json"),
];

/// 由声明式 manifest 驱动的内置 Adapter。
#[derive(Debug, Clone)]
pub struct BuiltinAdapter {
    manifest: AdapterManifest,
}

impl BuiltinAdapter {
    /// 按稳定 ID 加载一个内置 Adapter。
    pub fn load(adapter_id: &str) -> AdapterResult<Self> {
        let index = BUILTIN_ADAPTER_IDS
            .iter()
            .position(|candidate| *candidate == adapter_id)
            .ok_or_else(|| {
                AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    format!("未知内置 Adapter：{adapter_id}"),
                )
            })?;
        let manifest: AdapterManifest =
            serde_json::from_str(MANIFESTS[index]).map_err(|error| {
                AdapterError::new(
                    AdapterErrorCode::InvalidManifest,
                    format!("内置 manifest 无法解析：{error}"),
                )
            })?;
        manifest.validate()?;
        Ok(Self { manifest })
    }

    /// 加载全部首发 Adapter；任一内置 manifest 损坏都会阻止启动。
    pub fn load_all() -> AdapterResult<Vec<Self>> {
        BUILTIN_ADAPTER_IDS
            .iter()
            .map(|adapter_id| Self::load(adapter_id))
            .collect()
    }

    fn surface_for<'a>(
        &self,
        instance: &'a AgentInstance,
        kind: AssetKind,
        scope: &str,
        allow_manual: bool,
    ) -> Option<&'a NativeSurface> {
        let mut candidates: Vec<_> = instance
            .surfaces
            .iter()
            .filter(|surface| {
                surface.asset_kind == kind
                    && surface.scope == scope
                    && (surface.writable
                        || (allow_manual && surface.mode == SurfaceMode::ManualRequired))
            })
            .collect();
        candidates.sort_by_key(|surface| surface.precedence);
        candidates.into_iter().next()
    }
}

impl AgentAdapter for BuiltinAdapter {
    fn describe(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn detect(&self, context: &DetectionContext) -> AdapterResult<Vec<AgentInstance>> {
        if !self.manifest.supports_current_platform() {
            return Err(AdapterError::new(
                AdapterErrorCode::UnsupportedPlatform,
                "Adapter 不支持当前平台",
            ));
        }
        validate_context(context)?;
        // 每条 detection rule 可以代表不同 profile（例如全局与项目）。旧实现用
        // `any()` 把它们压成一个实例，会让 UI 无法分别选择目标。这里先按
        // `(profile, instance_root)` 去重：Devin 的 `.git` 与 `.agents` 两个 marker
        // 同属一个 repository profile，不会错误地产生两个相同实例。
        let mut detected_profiles = BTreeSet::new();
        for rule in &self.manifest.detection {
            let Some(marker) = expand_root(&rule.path, context)? else {
                continue;
            };
            if marker.exists() {
                let root = if rule.profile.is_some() {
                    context.project_root.clone().ok_or_else(|| {
                        AdapterError::new(
                            AdapterErrorCode::ValidationFailed,
                            "项目 profile 缺少 project_root",
                        )
                    })?
                } else {
                    context.home.clone()
                };
                detected_profiles.insert((rule.profile.clone(), root));
            }
        }
        if detected_profiles.is_empty() {
            return Ok(Vec::new());
        }

        detected_profiles
            .into_iter()
            .map(|(profile, instance_root)| {
                let mut surfaces = Vec::new();
                let mut roots = BTreeSet::new();
                for descriptor in &self.manifest.surfaces {
                    let belongs_to_profile = match &profile {
                        None => descriptor.scope == "global",
                        Some(_) => descriptor.scope != "global",
                    };
                    if !belongs_to_profile {
                        continue;
                    }
                    let Some(root) = expand_root(&descriptor.root, context)? else {
                        continue;
                    };
                    roots.insert(root.clone());
                    surfaces.push(NativeSurface {
                        id: descriptor.id.clone(),
                        scope: descriptor.scope.clone(),
                        asset_kind: descriptor.asset_kind,
                        root_path: root,
                        target_template: descriptor.target.clone(),
                        native_format: descriptor.native_format.clone(),
                        section: descriptor.section.clone(),
                        mode: descriptor.mode,
                        writable: descriptor.writable,
                        precedence: descriptor.precedence,
                    });
                }
                // Hermes/Pi 允许用户在原生配置中声明额外 Skill 路径。它们不是
                // RigDeck 的默认安装目标，但必须进入扫描和 watcher 根目录，才能把
                // Agent 原地编辑解释成 drift，而不是下次启动时静默丢失。
                for surface in configured_skill_surfaces(
                    &self.manifest.adapter_id,
                    context,
                    profile.as_deref(),
                )? {
                    roots.insert(surface.root_path.clone());
                    surfaces.push(surface);
                }
                let profile_id = profile.as_deref().unwrap_or("global");
                let identity_material = format!(
                    "{}\0{}\0{}",
                    self.manifest.adapter_id, profile_id, instance_root
                );
                Ok(AgentInstance {
                    id: ContentHash::from_bytes(identity_material.as_bytes()).to_string(),
                    adapter_id: self.manifest.adapter_id.clone(),
                    display_name: self.manifest.display_name.clone(),
                    version: detect_cli_version(&self.manifest.adapter_id),
                    managed_roots: roots.into_iter().collect(),
                    profile,
                    health: AgentHealth::Healthy,
                    surfaces,
                })
            })
            .collect()
    }

    fn scan(&self, instance: &AgentInstance) -> AdapterResult<Vec<ScannedEntry>> {
        ensure_instance(self, instance)?;
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        for surface in &instance.surfaces {
            let scan_root = scan_root(surface)?;
            for path in collect_files(&scan_root)? {
                if !seen.insert((path.clone(), surface.asset_kind)) {
                    continue;
                }
                let bytes = fs::read(&path).map_err(|error| io_error(&path, error))?;
                let managed = contains_managed_marker(&bytes);
                let (logical_id, semantic_issue) = scanned_identity(
                    &self.manifest.adapter_id,
                    surface,
                    &scan_root,
                    &path,
                    &bytes,
                );
                let structural_issue = if managed && surface.mode == SurfaceMode::ManagedBlock {
                    match classify_managed_document(&bytes) {
                        Ok(()) => None,
                        Err(ManagedBlockIssue::Duplicated) => {
                            Some(ObservationIssue::PromptBlockMoved)
                        }
                        Err(ManagedBlockIssue::Damaged) => {
                            Some(ObservationIssue::DamagedManagedBlock)
                        }
                    }
                } else {
                    None
                };
                entries.push(ScannedEntry {
                    path,
                    kind: surface.asset_kind,
                    raw_hash: ContentHash::from_bytes(&bytes),
                    normalized_hash: normalized_hash(&bytes),
                    managed,
                    logical_id,
                    // 资产语义问题比共享文档结构问题更靠近具体文件；两者不会出现在
                    // 同一种 surface 上，因此 `or` 不会隐藏另一项可修复错误。
                    issue: semantic_issue.or(structural_issue),
                });
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    fn validate_asset(
        &self,
        asset: &Asset,
        revision: &AssetRevision,
        instance: &AgentInstance,
        scope: &str,
    ) -> AdapterResult<AssetValidation> {
        ensure_instance(self, instance)?;
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        if asset.kind != revision.spec.kind() {
            errors.push("资产 kind 与 revision spec 不一致".to_owned());
        }
        let capability = self.manifest.capabilities.iter().any(|capability| {
            capability.asset_kind == asset.kind
                && capability.scopes.iter().any(|item| item == scope)
        });
        if !capability {
            errors.push(format!(
                "Adapter 不支持 {scope} 作用域的 {:?} 资产",
                asset.kind
            ));
        }
        match self.surface_for(instance, asset.kind, scope, true) {
            Some(surface) if surface.mode == SurfaceMode::ManualRequired => {
                errors.push("manual_required：官方没有等价的可写本地表面".to_owned());
            }
            Some(_) => {}
            None => errors.push("没有匹配的可写原生 surface".to_owned()),
        }
        if let AssetSpec::Prompt(prompt) = &revision.spec {
            if prompt.activation_condition.is_some() {
                warnings.push("目标可能无法完整表达条件激活语义，计划必须展示兼容损失".to_owned());
            }
        }
        if let AssetSpec::McpServer(server) = &revision.spec {
            if server.server_name != asset.identity.declared_name {
                errors.push(
                    "MCP server_name 必须与资产 declared_name 一致，才能保证精确卸载".to_owned(),
                );
            }
        }
        Ok(AssetValidation {
            valid: errors.is_empty(),
            warnings,
            errors,
        })
    }

    fn render(
        &self,
        asset: &Asset,
        revision: &AssetRevision,
        content: &[u8],
        instance: &AgentInstance,
        scope: &str,
        secret_bindings: &[SecretRef],
    ) -> AdapterResult<RenderOutput> {
        ensure_instance(self, instance)?;
        verify_hashes(content, &revision.raw_hash, &revision.normalized_hash)?;
        let surface = self
            .surface_for(instance, asset.kind, scope, true)
            .ok_or_else(|| {
                AdapterError::new(
                    AdapterErrorCode::UnsupportedCapability,
                    format!("没有 {:?}/{scope} 的投影 surface", asset.kind),
                )
            })?;

        if surface.mode == SurfaceMode::ManualRequired {
            return Ok(RenderOutput {
                projection: Projection {
                    revision_id: revision.id.clone(),
                    agent_instance_id: instance.id.clone(),
                    files: Vec::new(),
                    compatibility_losses: vec![CompatibilityLoss {
                        code: "manual_required".to_owned(),
                        message: "官方没有可安全自动写入的本地表面，需要人工交接".to_owned(),
                        blocking: true,
                    }],
                },
                objects: Vec::new(),
            });
        }

        let declared_name = safe_target_name(&asset.identity.declared_name)?;
        let target = render_target(surface, declared_name)?;
        let (bytes, strategy, losses) = match (&revision.spec, surface.mode) {
            (AssetSpec::Skill(skill), SurfaceMode::DirectoryTree) => (
                content.to_vec(),
                ProjectionStrategy::DirectoryTree {
                    relative_path: skill.entry_path.clone(),
                },
                Vec::new(),
            ),
            (AssetSpec::Prompt(prompt), SurfaceMode::ManagedBlock) => {
                let mut losses = Vec::new();
                if prompt.activation_condition.is_some() {
                    losses.push(CompatibilityLoss {
                        code: "conditional_activation_flattened".to_owned(),
                        message: "该目标只支持普通托管块，条件激活会被展平".to_owned(),
                        blocking: false,
                    });
                }
                return render_single(
                    asset,
                    revision,
                    instance,
                    surface,
                    target,
                    content.to_vec(),
                    ProjectionStrategy::ManagedBlock {
                        block_id: asset.id.clone(),
                    },
                    losses,
                );
            }
            (AssetSpec::Prompt(_), SurfaceMode::ReplaceFile) => (
                content.to_vec(),
                ProjectionStrategy::ReplaceFile,
                Vec::new(),
            ),
            (AssetSpec::McpServer(server), SurfaceMode::StructuredEntry) => {
                validate_secret_bindings(server, secret_bindings)?;
                let section = surface_section(surface)?;
                let codec_id = self
                    .manifest
                    .codecs
                    .iter()
                    .find(|codec| {
                        codec.asset_kind == AssetKind::McpServer
                            && codec.native_format == surface.native_format
                    })
                    .map(|codec| codec.id.as_str())
                    .ok_or_else(|| {
                        AdapterError::new(
                            AdapterErrorCode::InvalidManifest,
                            format!("surface {} 缺少 MCP codec", surface.id),
                        )
                    })?;
                let encoded = encode_mcp_entry(codec_id, server)?;
                (
                    encoded.bytes,
                    ProjectionStrategy::StructuredEntry {
                        section: section.to_owned(),
                        entry_key: server.server_name.clone(),
                    },
                    encoded.compatibility_losses,
                )
            }
            _ => {
                return Err(AdapterError::new(
                    AdapterErrorCode::UnsupportedCapability,
                    "资产 spec 与 surface 投影方式不匹配",
                ));
            }
        };
        render_single(
            asset, revision, instance, surface, target, bytes, strategy, losses,
        )
    }

    fn render_bundle(
        &self,
        asset: &Asset,
        revision: &AssetRevision,
        content: &AssetContent,
        instance: &AgentInstance,
        scope: &str,
        secret_bindings: &[SecretRef],
    ) -> AdapterResult<RenderOutput> {
        let AssetSpec::Skill(skill) = &revision.spec else {
            let [file] = content.files.as_slice() else {
                return Err(AdapterError::new(
                    AdapterErrorCode::ValidationFailed,
                    "Prompt/MCP 资产必须只有一个主内容文件",
                ));
            };
            return self.render(
                asset,
                revision,
                &file.bytes,
                instance,
                scope,
                secret_bindings,
            );
        };
        ensure_instance(self, instance)?;
        if asset.kind != AssetKind::Skill || content.files.is_empty() || content.files.len() > 1_000
        {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "Skill bundle 必须包含 1..=1000 个文件",
            ));
        }
        let total_bytes: usize = content.files.iter().map(|file| file.bytes.len()).sum();
        if total_bytes > 64 * 1024 * 1024 {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "Skill bundle 总大小超过 64 MiB",
            ));
        }
        if content.raw_hash()? != revision.raw_hash
            || content.normalized_hash()? != revision.normalized_hash
        {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "Skill bundle hash 与不可变 revision 不一致",
            ));
        }
        let inventory = content.inventory_bytes()?;
        if ContentHash::from_bytes(&inventory) != skill.inventory_object {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "Skill 文件清单与 inventory_object 不一致",
            ));
        }
        let entry = content
            .files
            .iter()
            .find(|file| file.relative_path == skill.entry_path)
            .ok_or_else(|| {
                AdapterError::new(
                    AdapterErrorCode::ValidationFailed,
                    format!("Skill bundle 缺少入口文件：{}", skill.entry_path),
                )
            })?;
        if ContentHash::from_bytes(&entry.bytes) != revision.content_object {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "Skill 入口文件与 revision.content_object 不一致",
            ));
        }
        let surface = self
            .surface_for(instance, AssetKind::Skill, scope, false)
            .ok_or_else(|| {
                AdapterError::new(
                    AdapterErrorCode::UnsupportedCapability,
                    "没有可写 Skill directory surface",
                )
            })?;
        if surface.mode != SurfaceMode::DirectoryTree {
            return Err(AdapterError::new(
                AdapterErrorCode::InvalidManifest,
                "Skill bundle 必须投影到 directory_tree surface",
            ));
        }
        let target = render_target(surface, safe_target_name(&asset.identity.declared_name)?)?;
        let mut files = Vec::with_capacity(content.files.len());
        let mut objects = Vec::with_capacity(content.files.len());
        for source in &content.files {
            let hash = ContentHash::from_bytes(&source.bytes);
            files.push(ProjectedFile {
                target_path: target.clone(),
                content_object: hash.clone(),
                raw_hash: hash.clone(),
                native_format: surface.native_format.clone(),
                strategy: ProjectionStrategy::DirectoryTree {
                    relative_path: source.relative_path.clone(),
                },
            });
            objects.push(RenderedObject {
                hash,
                bytes: source.bytes.clone(),
            });
        }
        files.sort_by(|left, right| match (&left.strategy, &right.strategy) {
            (
                ProjectionStrategy::DirectoryTree {
                    relative_path: left,
                },
                ProjectionStrategy::DirectoryTree {
                    relative_path: right,
                },
            ) => left.cmp(right),
            _ => std::cmp::Ordering::Equal,
        });
        Ok(RenderOutput {
            projection: Projection {
                revision_id: revision.id.clone(),
                agent_instance_id: instance.id.clone(),
                files,
                compatibility_losses: Vec::new(),
            },
            objects,
        })
    }

    fn plan_install(&self, projection: &Projection) -> AdapterResult<AdapterPlan> {
        plan_projection(projection)
    }

    fn plan_update(&self, projection: &Projection) -> AdapterResult<AdapterPlan> {
        plan_projection(projection)
    }

    fn plan_remove(
        &self,
        asset: &Asset,
        instance: &AgentInstance,
        scope: &str,
    ) -> AdapterResult<AdapterPlan> {
        ensure_instance(self, instance)?;
        let surface = self
            .surface_for(instance, asset.kind, scope, true)
            .ok_or_else(|| {
                AdapterError::new(
                    AdapterErrorCode::UnsupportedCapability,
                    "没有可移除的原生 surface",
                )
            })?;
        if surface.mode == SurfaceMode::ManualRequired {
            return Ok(AdapterPlan {
                projection: None,
                remove_targets: Vec::new(),
                removals: Vec::new(),
                manual_required: vec![
                    "在官方界面或公开 API 中移除对应配置，然后运行 refresh 验证".to_owned()
                ],
            });
        }
        let name = safe_target_name(&asset.identity.declared_name)?;
        let target = render_target(surface, name)?;
        let strategy = match surface.mode {
            SurfaceMode::ReplaceFile | SurfaceMode::DirectoryTree => {
                RemovalStrategy::RemoveOwnedPath
            }
            SurfaceMode::ManagedBlock => RemovalStrategy::ManagedBlock {
                block_id: asset.id.clone(),
            },
            SurfaceMode::StructuredEntry => RemovalStrategy::StructuredEntry {
                section: surface_section(surface)?.to_owned(),
                entry_key: asset.identity.declared_name.clone(),
            },
            SurfaceMode::ManualRequired => unreachable!("前面已返回"),
        };
        Ok(AdapterPlan {
            projection: None,
            remove_targets: matches!(strategy, RemovalStrategy::RemoveOwnedPath)
                .then(|| target.clone())
                .into_iter()
                .collect(),
            removals: vec![RemovalIntent {
                target_path: target,
                strategy,
            }],
            manual_required: Vec::new(),
        })
    }

    fn verify(&self, projection: &Projection) -> AdapterResult<VerificationReport> {
        let mut differences = Vec::new();
        for file in &projection.files {
            match &file.strategy {
                ProjectionStrategy::ReplaceFile => match fs::read(&file.target_path) {
                    Ok(bytes) if ContentHash::from_bytes(&bytes) == file.raw_hash => {}
                    Ok(_) => {
                        differences.push(format!("文件内容 hash 不一致：{}", file.target_path))
                    }
                    Err(error) => {
                        differences.push(format!("无法读取 {}：{error}", file.target_path))
                    }
                },
                ProjectionStrategy::DirectoryTree { relative_path } => {
                    let target = file.target_path.join(relative_path);
                    match fs::read(&target) {
                        Ok(bytes) if ContentHash::from_bytes(&bytes) == file.raw_hash => {}
                        Ok(_) => differences.push(format!("Skill 主文件 hash 不一致：{target}")),
                        Err(error) => {
                            differences.push(format!("无法读取 Skill 主文件 {target}：{error}"))
                        }
                    }
                }
                ProjectionStrategy::ManagedBlock { block_id } => {
                    match fs::read(&file.target_path) {
                        Ok(bytes) => match inspect_managed_block(&bytes, block_id) {
                            Ok(ManagedBlockState::Present { revision })
                                if revision == projection.revision_id => {}
                            Ok(_) => {
                                differences.push(format!("托管块修订不一致：{}", file.target_path))
                            }
                            Err(error) => differences.push(error.to_string()),
                        },
                        Err(error) => {
                            differences.push(format!("无法读取 {}：{error}", file.target_path))
                        }
                    }
                }
                ProjectionStrategy::StructuredEntry { section, entry_key } => {
                    match fs::read_to_string(&file.target_path) {
                        Ok(text) if text.contains(section) && text.contains(entry_key) => {}
                        Ok(_) => differences.push(format!("结构化条目缺失：{section}.{entry_key}")),
                        Err(error) => {
                            differences.push(format!("无法读取 {}：{error}", file.target_path))
                        }
                    }
                }
            }
        }
        Ok(VerificationReport {
            valid: differences.is_empty(),
            differences,
        })
    }

    fn health(&self, context: &DetectionContext) -> AdapterResult<AdapterHealthReport> {
        self.manifest.validate()?;
        validate_context(context)?;
        let detected = self.detect(context)?;
        Ok(AdapterHealthReport {
            healthy: !detected.is_empty(),
            checks: vec![
                "manifest 语义有效".to_owned(),
                if detected.is_empty() {
                    "未检测到本地实例".to_owned()
                } else {
                    "本地实例与管理表面可解析".to_owned()
                },
            ],
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn render_single(
    _asset: &Asset,
    revision: &AssetRevision,
    instance: &AgentInstance,
    surface: &NativeSurface,
    target: Utf8PathBuf,
    bytes: Vec<u8>,
    strategy: ProjectionStrategy,
    losses: Vec<CompatibilityLoss>,
) -> AdapterResult<RenderOutput> {
    let hash = ContentHash::from_bytes(&bytes);
    Ok(RenderOutput {
        projection: Projection {
            revision_id: revision.id.clone(),
            agent_instance_id: instance.id.clone(),
            files: vec![ProjectedFile {
                target_path: target,
                content_object: hash.clone(),
                raw_hash: hash.clone(),
                native_format: surface.native_format.clone(),
                strategy,
            }],
            compatibility_losses: losses,
        },
        objects: vec![RenderedObject { hash, bytes }],
    })
}

fn plan_projection(projection: &Projection) -> AdapterResult<AdapterPlan> {
    let manual_required = projection
        .compatibility_losses
        .iter()
        .filter(|loss| loss.blocking)
        .map(|loss| loss.message.clone())
        .collect();
    Ok(AdapterPlan {
        projection: Some(projection.clone()),
        remove_targets: Vec::new(),
        removals: Vec::new(),
        manual_required,
    })
}

fn ensure_instance(adapter: &BuiltinAdapter, instance: &AgentInstance) -> AdapterResult<()> {
    if instance.adapter_id != adapter.manifest.adapter_id {
        return Err(AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!(
                "实例属于 {}，不能交给 {} Adapter",
                instance.adapter_id, adapter.manifest.adapter_id
            ),
        ));
    }
    Ok(())
}

fn validate_context(context: &DetectionContext) -> AdapterResult<()> {
    if !context.home.is_absolute()
        || context
            .home
            .components()
            .any(|part| matches!(part, Utf8Component::ParentDir))
    {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "home 必须是无 `..` 的绝对路径",
        ));
    }
    if let Some(project) = &context.project_root {
        if !project.is_absolute()
            || project
                .components()
                .any(|part| matches!(part, Utf8Component::ParentDir))
        {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                "project root 必须是无 `..` 的绝对路径",
            ));
        }
    }
    Ok(())
}

fn expand_root(template: &str, context: &DetectionContext) -> AdapterResult<Option<Utf8PathBuf>> {
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
            "surface root 缺少允许的根 token",
        ));
    };
    let suffix = suffix.trim_start_matches(['/', '\\']);
    let path = root.join(suffix);
    if path
        .components()
        .any(|part| matches!(part, Utf8Component::ParentDir))
    {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "展开后的 surface root 包含 `..`",
        ));
    }
    Ok(Some(path))
}

fn safe_target_name(name: &str) -> AdapterResult<&str> {
    let trimmed = name.trim();
    let reserved = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    // Windows 会把 `CON.txt` 仍视为设备名，所以按第一个点之前的 basename 判断；
    // 即使当前运行在 macOS，也使用同一规则，避免同一资产跨平台后才变成不可安装。
    let device_basename = trimmed.split('.').next().unwrap_or(trimmed);
    let valid = name == trimmed
        && !trimmed.is_empty()
        && trimmed.len() <= 120
        && trimmed != "."
        && trimmed != ".."
        && !trimmed.contains([
            '<', '>', ':', '"', '/', '\\', '|', '?', '*', '\0', '\r', '\n',
        ])
        && !trimmed.chars().any(|character| character <= '\u{1f}')
        && !trimmed.ends_with(['.', ' '])
        && !reserved
            .iter()
            .any(|item| device_basename.eq_ignore_ascii_case(item));
    if !valid {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("资产声明名不能安全映射为目标路径：{name}"),
        ));
    }
    Ok(trimmed)
}

/// 从原生文件路径推导稳定逻辑 ID，并识别声明名与目录名不一致。
///
/// `Option` 表示某些共享配置文件不能无损对应到单一资产；此时宁可不猜，也不能把
/// 整份配置错误导入成一个 Skill。该函数只读取字节，不执行 Skill 中的任何内容。
fn scanned_identity(
    adapter_id: &str,
    surface: &NativeSurface,
    scan_root: &Utf8Path,
    path: &Utf8Path,
    bytes: &[u8],
) -> (Option<String>, Option<ObservationIssue>) {
    if surface.asset_kind == AssetKind::Skill && surface.mode == SurfaceMode::DirectoryTree {
        let Some(skill_root) = nearest_skill_root(scan_root, path) else {
            return (None, Some(ObservationIssue::PathAnomaly));
        };
        let Some(directory) = skill_root.file_name() else {
            return (None, Some(ObservationIssue::PathAnomaly));
        };
        let mut issue = safe_target_name(directory)
            .err()
            .map(|_| ObservationIssue::PathAnomaly);
        if path.file_name() == Some("SKILL.md") {
            if let Some(declared) = skill_frontmatter_name(bytes) {
                // Pi 官方明确允许名称与目录不同；其他首发 Agent 按 Agent Skills
                // 标准把不一致展示为可解释冲突。
                if adapter_id != "pi" && declared != directory {
                    issue = Some(ObservationIssue::DeclaredNameMismatch);
                }
            }
        }
        return (Some(format!("skill:{}", directory.to_lowercase())), issue);
    }

    if surface.asset_kind == AssetKind::Skill && surface.mode == SurfaceMode::ReplaceFile {
        let declared = skill_frontmatter_name(bytes);
        let fallback = path.file_stem().map(ToOwned::to_owned);
        let Some(name) = declared.or(fallback) else {
            return (None, Some(ObservationIssue::PathAnomaly));
        };
        let issue = safe_target_name(&name)
            .err()
            .map(|_| ObservationIssue::PathAnomaly);
        return (Some(format!("skill:{}", name.to_lowercase())), issue);
    }

    if surface.asset_kind == AssetKind::Prompt
        && surface.mode == SurfaceMode::ReplaceFile
        && surface.target_template.contains("{name}")
    {
        let Some(stem) = path.file_stem() else {
            return (None, Some(ObservationIssue::PathAnomaly));
        };
        let issue = safe_target_name(stem)
            .err()
            .map(|_| ObservationIssue::PathAnomaly);
        return (Some(format!("prompt:{}", stem.to_lowercase())), issue);
    }

    (None, None)
}

/// 找到包含当前文件的最近一层 Skill 根目录。
///
/// Hermes 支持 `category/name/SKILL.md`，Pi 的配置项也可以直接指向某个 Skill 根目录；
/// 因此不能假设扫描根目录的第一段就是 Skill 名称。
fn nearest_skill_root<'a>(scan_root: &'a Utf8Path, path: &'a Utf8Path) -> Option<&'a Utf8Path> {
    let start = if path.is_dir() { path } else { path.parent()? };
    start
        .ancestors()
        .take_while(|candidate| candidate.starts_with(scan_root))
        .find(|candidate| candidate.join("SKILL.md").is_file())
}

/// 读取 Agent Skill YAML frontmatter 中的 `name`。
///
/// 这里只反序列化首个 YAML 文档。返回拥有所有权的 `String`，是因为解析出的 YAML
/// `Value` 会在函数结束时释放；若返回指向它的 `&str`，Rust 会在编译期拒绝悬空借用。
/// 格式损坏时返回 `None`，由资产导入审计给出更详细错误。
fn skill_frontmatter_name(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    let mut lines = text.lines();
    if lines.next()?.trim_end_matches('\r') != "---" {
        return None;
    }
    let mut yaml = String::new();
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line == "---" {
            let value: serde_yaml::Value = serde_yaml::from_str(&yaml).ok()?;
            return value
                .as_mapping()?
                .get(serde_yaml::Value::String("name".to_owned()))?
                .as_str()
                .map(ToOwned::to_owned);
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    None
}

fn render_target(surface: &NativeSurface, name: &str) -> AdapterResult<Utf8PathBuf> {
    let relative = surface.target_template.replace("{name}", name);
    if relative.contains('{') || relative.contains('}') {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "目标模板包含未知变量",
        ));
    }
    let relative = Utf8Path::new(&relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| matches!(part, Utf8Component::ParentDir | Utf8Component::Prefix(_)))
    {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "目标模板逃逸 surface root",
        ));
    }
    Ok(surface.root_path.join(relative))
}

/// 读取 Agent 原生配置中声明的额外 Skill 路径。
///
/// 返回的 surface 均为只读：RigDeck 会扫描并监听它们，但新安装仍写入 manifest 中
/// `precedence = 0` 的首选目录。这样既能观察 Hermes/Pi 的原地修改，又不会把一个共享
/// Skill 路径意外变成 RigDeck 独占目录。
fn configured_skill_surfaces(
    adapter_id: &str,
    context: &DetectionContext,
    profile: Option<&str>,
) -> AdapterResult<Vec<NativeSurface>> {
    let configured = match (adapter_id, profile) {
        ("hermes", None) => hermes_external_skill_paths(context)?,
        ("pi", None) => pi_configured_skill_paths(
            &context.home.join(".pi/agent/settings.json"),
            &context.home.join(".pi/agent"),
            &context.home,
        )?,
        ("pi", Some(_)) => {
            let Some(project) = &context.project_root else {
                return Ok(Vec::new());
            };
            pi_configured_skill_paths(
                &project.join(".pi/settings.json"),
                &project.join(".pi"),
                &context.home,
            )?
        }
        _ => Vec::new(),
    };
    let scope = if profile.is_some() {
        "project"
    } else {
        "global"
    };
    configured
        .into_iter()
        .enumerate()
        .map(|(index, path)| configured_skill_surface(path, scope, index))
        .collect()
}

fn hermes_external_skill_paths(context: &DetectionContext) -> AdapterResult<Vec<Utf8PathBuf>> {
    let config_path = context.home.join(".hermes/config.yaml");
    let Some(bytes) = read_optional_config(&config_path)? else {
        return Ok(Vec::new());
    };
    let value: serde_yaml::Value = serde_yaml::from_slice(&bytes).map_err(|error| {
        AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!("Hermes config.yaml 无法解析：{error}"),
        )
    })?;
    let Some(external_dirs) = value
        .get("skills")
        .and_then(|skills| skills.get("external_dirs"))
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return Ok(Vec::new());
    };
    let base = config_path.parent().unwrap_or(&context.home);
    let mut paths = BTreeSet::new();
    for value in external_dirs {
        let Some(raw) = value.as_str() else {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "Hermes skills.external_dirs 必须只包含字符串路径",
            ));
        };
        let Some(expanded) = expand_config_pattern(raw, base, &context.home)? else {
            // 官方语义是不存在的可选目录可静默跳过；缺失环境变量也按未配置处理。
            continue;
        };
        if let Some(path) = canonical_existing(&expanded)? {
            paths.insert(path);
        }
    }
    Ok(paths.into_iter().collect())
}

fn pi_configured_skill_paths(
    config_path: &Utf8Path,
    base: &Utf8Path,
    home: &Utf8Path,
) -> AdapterResult<Vec<Utf8PathBuf>> {
    let Some(bytes) = read_optional_config(config_path)? else {
        return Ok(Vec::new());
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!("Pi settings.json 无法解析：{error}"),
        )
    })?;
    let Some(entries) = value.get("skills").and_then(serde_json::Value::as_array) else {
        return Ok(Vec::new());
    };

    let mut included = BTreeSet::new();
    let mut forced = BTreeSet::new();
    let mut excluded = BTreeSet::new();
    for entry in entries {
        let Some(raw) = entry.as_str() else {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "Pi settings.json 的 skills 必须只包含字符串路径",
            ));
        };
        let (mode, raw) = match raw.as_bytes().first().copied() {
            Some(b'!') => ("exclude", &raw[1..]),
            Some(b'+') => ("force", &raw[1..]),
            Some(b'-') => ("exclude", &raw[1..]),
            _ => ("include", raw),
        };
        let Some(pattern) = expand_config_pattern(raw, base, home)? else {
            continue;
        };
        if mode == "exclude" {
            excluded.extend(expand_existing_matches(&pattern)?);
            continue;
        }

        let matches = expand_existing_matches(&pattern)?;
        if mode == "force" {
            forced.extend(matches);
        } else {
            included.extend(matches);
        }
    }
    included.retain(|path| !excluded.contains(path));
    included.extend(forced);
    Ok(included.into_iter().collect())
}

fn configured_skill_surface(
    path: Utf8PathBuf,
    scope: &str,
    index: usize,
) -> AdapterResult<NativeSurface> {
    let metadata = fs::metadata(&path).map_err(|error| io_error(&path, error))?;
    let identity = ContentHash::from_bytes(path.as_str().as_bytes());
    let (root_path, target_template, mode, native_format) = if metadata.is_dir() {
        (
            path,
            "{name}".to_owned(),
            SurfaceMode::DirectoryTree,
            "configured-skill-directory".to_owned(),
        )
    } else if metadata.is_file() {
        let parent = path.parent().ok_or_else(|| {
            AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("配置的 Skill 文件没有父目录：{path}"),
            )
        })?;
        let filename = path.file_name().ok_or_else(|| {
            AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("配置的 Skill 文件名无效：{path}"),
            )
        })?;
        (
            parent.to_owned(),
            filename.to_owned(),
            SurfaceMode::ReplaceFile,
            "configured-skill-file".to_owned(),
        )
    } else {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("配置的 Skill 路径既不是文件也不是目录：{path}"),
        ));
    };
    Ok(NativeSurface {
        id: format!("configured-skill-{index}-{}", identity.as_str()),
        scope: scope.to_owned(),
        asset_kind: AssetKind::Skill,
        root_path,
        target_template,
        native_format,
        section: None,
        mode,
        writable: false,
        precedence: 1_000u16.saturating_add(u16::try_from(index).unwrap_or(u16::MAX - 1_000)),
    })
}

fn read_optional_config(path: &Utf8Path) -> AdapterResult<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(path, error)),
    }
}

fn expand_config_pattern(
    raw: &str,
    base: &Utf8Path,
    home: &Utf8Path,
) -> AdapterResult<Option<Utf8PathBuf>> {
    let Some(with_env) = expand_braced_environment(raw) else {
        return Ok(None);
    };
    let expanded = if with_env == "~" {
        home.to_owned()
    } else if let Some(relative) = with_env
        .strip_prefix("~/")
        .or_else(|| with_env.strip_prefix("~\\"))
    {
        home.join(relative)
    } else {
        let path = Utf8Path::new(&with_env);
        if path.is_absolute() {
            path.to_owned()
        } else {
            base.join(path)
        }
    };
    if expanded.components().any(|component| {
        matches!(
            component,
            Utf8Component::Prefix(_) | Utf8Component::RootDir | Utf8Component::Normal(_)
        )
    }) {
        Ok(Some(expanded))
    } else {
        Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            format!("配置的 Skill 路径无效：{raw}"),
        ))
    }
}

fn expand_braced_environment(raw: &str) -> Option<String> {
    let mut output = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let variable = &rest[start + 2..];
        let end = variable.find('}')?;
        let name = &variable[..end];
        if name.is_empty()
            || !name
                .chars()
                .all(|character| character == '_' || character.is_ascii_alphanumeric())
        {
            return None;
        }
        output.push_str(&std::env::var(name).ok()?);
        rest = &variable[end + 1..];
    }
    output.push_str(rest);
    Some(output)
}

fn expand_existing_matches(pattern: &Utf8Path) -> AdapterResult<Vec<Utf8PathBuf>> {
    let contains_glob = pattern.as_str().contains(['*', '?', '[']);
    if !contains_glob {
        return Ok(canonical_existing(pattern)?.into_iter().collect());
    }
    let mut matches = Vec::new();
    let paths = glob::glob(pattern.as_str()).map_err(|error| {
        AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!("Pi skills glob 无效：{error}"),
        )
    })?;
    for result in paths {
        let path = result.map_err(|error| {
            AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("Pi skills glob 无法读取路径：{error}"),
            )
        })?;
        let path = Utf8PathBuf::from_path_buf(path).map_err(|path| {
            AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("配置的 Skill 路径不是 UTF-8：{}", path.display()),
            )
        })?;
        if let Some(path) = canonical_existing(&path)? {
            matches.push(path);
        }
    }
    Ok(matches)
}

fn canonical_existing(path: &Utf8Path) -> AdapterResult<Option<Utf8PathBuf>> {
    let canonical = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(path, error)),
    };
    Utf8PathBuf::from_path_buf(canonical)
        .map(Some)
        .map_err(|path| {
            AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("配置的 Skill 路径不是 UTF-8：{}", path.display()),
            )
        })
}

fn surface_section(surface: &NativeSurface) -> AdapterResult<&str> {
    surface.section.as_deref().ok_or_else(|| {
        AdapterError::new(
            AdapterErrorCode::InvalidManifest,
            "结构化 surface 缺少 section",
        )
    })
}

fn scan_root(surface: &NativeSurface) -> AdapterResult<Utf8PathBuf> {
    let template = surface.target_template.as_str();
    let relative = if let Some(index) = template.find("{name}") {
        template[..index].trim_end_matches(['/', '\\'])
    } else {
        template
    };
    let relative = Utf8Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| matches!(part, Utf8Component::ParentDir | Utf8Component::Prefix(_)))
    {
        return Err(AdapterError::new(
            AdapterErrorCode::PathViolation,
            "扫描路径逃逸 surface root",
        ));
    }
    Ok(surface.root_path.join(relative))
}

fn collect_files(root: &Utf8Path) -> AdapterResult<Vec<Utf8PathBuf>> {
    const MAX_FILES: usize = 10_000;
    const MAX_DEPTH: usize = 32;
    const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut output = Vec::new();
    let mut pending = vec![(root.to_owned(), 0usize)];
    while let Some((path, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("扫描深度超过限制：{path}"),
            ));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("扫描拒绝 symlink：{path}"),
            ));
        }
        if metadata.is_file() {
            if metadata.len() > MAX_FILE_BYTES {
                return Err(AdapterError::new(
                    AdapterErrorCode::ValidationFailed,
                    format!("单文件超过 16 MiB 扫描限制：{path}"),
                ));
            }
            output.push(path);
            if output.len() > MAX_FILES {
                return Err(AdapterError::new(
                    AdapterErrorCode::ValidationFailed,
                    "单实例扫描文件数超过 10,000",
                ));
            }
        } else if metadata.is_dir() {
            for entry in fs::read_dir(&path).map_err(|error| io_error(&path, error))? {
                let entry = entry.map_err(|error| io_error(&path, error))?;
                let child = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                    AdapterError::new(
                        AdapterErrorCode::PathViolation,
                        format!("扫描路径不是 UTF-8：{}", path.display()),
                    )
                })?;
                pending.push((child, depth + 1));
            }
        }
    }
    Ok(output)
}

fn validate_secret_bindings(
    server: &rigdeck_core::McpServerSpec,
    supplied: &[SecretRef],
) -> AdapterResult<()> {
    let referenced: Vec<&SecretRef> = match &server.transport {
        McpTransport::Stdio { env, .. } => env.values().filter_map(secret_ref).collect(),
        McpTransport::StreamableHttp { headers, .. } => {
            headers.values().filter_map(secret_ref).collect()
        }
    };
    let missing: Vec<_> = referenced
        .into_iter()
        .filter(|reference| !supplied.contains(reference))
        .map(|reference| reference.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!("缺少 MCP SecretRef 绑定：{}", missing.join(", ")),
        ));
    }
    Ok(())
}

fn secret_ref(value: &BindingValue) -> Option<&SecretRef> {
    match value {
        BindingValue::Secret(reference) => Some(reference),
        BindingValue::Literal(_) => None,
    }
}

fn io_error(path: &Utf8Path, error: std::io::Error) -> AdapterError {
    AdapterError::new(
        AdapterErrorCode::Io,
        format!("文件系统错误（{path}）：{error}"),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use rigdeck_core::{
        AssetIdentity, AssetState, AuditResult, McpServerSpec, PromptSpec, SkillSpec, Source,
        SourceKind,
    };
    use serde::Deserialize;

    #[test]
    fn extract_version_number_parses_common_formats() {
        assert_eq!(
            extract_version_number("2.1.165 (Claude Code)"),
            Some("2.1.165".to_owned())
        );
        assert_eq!(
            extract_version_number("codex-cli 0.144.1"),
            Some("0.144.1".to_owned())
        );
        assert_eq!(
            extract_version_number("1.14.20"),
            Some("1.14.20".to_owned())
        );
        assert_eq!(
            extract_version_number("devin 3000.1.27 (0d4bf12e)"),
            Some("3000.1.27".to_owned())
        );
        assert_eq!(extract_version_number("no version here"), None);
        assert_eq!(extract_version_number(""), None);
    }

    fn context(root: &Utf8Path) -> DetectionContext {
        DetectionContext {
            home: root.join("home"),
            project_root: Some(root.join("project")),
        }
    }

    fn prompt_asset(content: &[u8]) -> (Asset, AssetRevision) {
        let identity = AssetIdentity::new("local", "fixture", ".", "review-rules").unwrap();
        let asset = Asset {
            id: identity.stable_id(),
            identity,
            kind: AssetKind::Prompt,
            display_name: "Review Rules".to_owned(),
            current_revision_id: Some("rev-1".to_owned()),
            state: AssetState::Active,
            tags: Vec::new(),
        };
        let hash = ContentHash::from_bytes(content);
        let revision = AssetRevision {
            id: "rev-1".to_owned(),
            raw_hash: hash.clone(),
            normalized_hash: normalized_hash(content),
            content_object: hash.clone(),
            source: Source {
                kind: SourceKind::LocalFolder,
                namespace: "fixture".to_owned(),
                locator: "fixture".to_owned(),
                revision: None,
            },
            license: None,
            audit: AuditResult {
                schema_version: 1,
                completed: true,
                findings: Vec::new(),
            },
            spec: AssetSpec::Prompt(PromptSpec {
                content_object: hash,
                order: 0,
                scopes: vec!["project".to_owned()],
                activation_condition: None,
            }),
            created_at_ms: 0,
            author: None,
            update_time_ms: None,
            platform_restrictions: Vec::new(),
        };
        (asset, revision)
    }

    fn skill_asset() -> (Asset, AssetRevision, AssetContent) {
        skill_asset_from(
            "golden-skill",
            b"---\nname: golden-skill\nlicense: MIT\n---\n# Golden Skill\n".to_vec(),
        )
    }

    fn skill_asset_from(name: &str, bytes: Vec<u8>) -> (Asset, AssetRevision, AssetContent) {
        let content = AssetContent::single("SKILL.md", bytes);
        let identity = AssetIdentity::new("local", "golden", ".", name).unwrap();
        let asset = Asset::new(identity, AssetKind::Skill);
        let inventory = content.inventory_bytes().unwrap();
        let entry_hash = ContentHash::from_bytes(&content.files[0].bytes);
        let revision = AssetRevision {
            id: "golden-skill-revision".to_owned(),
            raw_hash: content.raw_hash().unwrap(),
            normalized_hash: content.normalized_hash().unwrap(),
            content_object: entry_hash,
            source: Source {
                kind: SourceKind::LocalFolder,
                namespace: "golden".to_owned(),
                locator: "fixture".to_owned(),
                revision: None,
            },
            license: Some("MIT".to_owned()),
            audit: AuditResult {
                schema_version: 1,
                completed: true,
                findings: Vec::new(),
            },
            spec: AssetSpec::Skill(SkillSpec {
                entry_path: "SKILL.md".into(),
                inventory_object: ContentHash::from_bytes(&inventory),
                native_metadata: BTreeMap::new(),
            }),
            created_at_ms: 0,
            author: None,
            update_time_ms: None,
            platform_restrictions: Vec::new(),
        };
        (asset, revision, content)
    }

    fn mcp_asset() -> (Asset, AssetRevision) {
        let identity = AssetIdentity::new("local", "golden", ".", "golden-mcp").unwrap();
        let asset = Asset::new(identity, AssetKind::McpServer);
        let content = b"{}";
        let hash = ContentHash::from_bytes(content);
        let revision = AssetRevision {
            id: "golden-mcp-revision".to_owned(),
            raw_hash: hash.clone(),
            normalized_hash: normalized_hash(content),
            content_object: hash,
            source: Source {
                kind: SourceKind::LocalFolder,
                namespace: "golden".to_owned(),
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
                server_name: "golden-mcp".to_owned(),
                transport: McpTransport::Stdio {
                    command: "golden-server".to_owned(),
                    args: vec!["--stdio".to_owned()],
                    env: BTreeMap::new(),
                },
                enabled: true,
                timeout_ms: Some(30_000),
                oauth: None,
                allowed_tools: vec!["read".to_owned()],
                denied_tools: Vec::new(),
            }),
            created_at_ms: 0,
            author: None,
            update_time_ms: None,
            platform_restrictions: Vec::new(),
        };
        (asset, revision)
    }

    #[derive(Debug, Deserialize)]
    struct GoldenSuite {
        schema_version: u32,
        platform: String,
        reference_home: String,
        reference_project: String,
        line_ending: String,
        adapters: Vec<GoldenAdapter>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenAdapter {
        adapter_id: String,
        home_marker: Option<Utf8PathBuf>,
        project_marker: Option<Utf8PathBuf>,
        profiles: Vec<GoldenProfile>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenProfile {
        profile: Option<String>,
        scope: String,
        skill_target: Utf8PathBuf,
        prompt_target: Utf8PathBuf,
        mcp_target: Option<Utf8PathBuf>,
        mcp_mode: String,
    }

    #[derive(Debug, Deserialize)]
    struct MetaTargets {
        schema_version: u32,
        skill: String,
        targets: Vec<MetaTarget>,
    }

    #[derive(Debug, Deserialize)]
    struct MetaTarget {
        adapter_id: String,
        global: Option<Utf8PathBuf>,
        project: Utf8PathBuf,
    }

    fn run_golden_suite(json: &str) {
        let suite: GoldenSuite = serde_json::from_str(json).unwrap();
        assert_eq!(suite.schema_version, 1);
        assert!(matches!(suite.platform.as_str(), "windows" | "macos"));
        assert!(!suite.reference_home.is_empty());
        assert!(!suite.reference_project.is_empty());
        assert_eq!(suite.adapters.len(), BUILTIN_ADAPTER_IDS.len());
        let newline = match suite.line_ending.as_str() {
            "crlf" => "\r\n",
            "lf" => "\n",
            other => panic!("未知换行类型：{other}"),
        };

        for expected in suite.adapters {
            let temp = tempfile::tempdir().unwrap();
            let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
            let ctx = context(&root);
            fs::create_dir_all(&ctx.home).unwrap();
            fs::create_dir_all(ctx.project_root.as_ref().unwrap()).unwrap();
            if let Some(marker) = &expected.home_marker {
                fs::create_dir_all(ctx.home.join(marker)).unwrap();
            }
            if let Some(marker) = &expected.project_marker {
                fs::create_dir_all(ctx.project_root.as_ref().unwrap().join(marker)).unwrap();
            }

            // 先放入未托管原生内容，证明 scan 能从真实表面导入并保留平台换行。
            for profile in &expected.profiles {
                let base = if profile.profile.is_none() {
                    &ctx.home
                } else {
                    ctx.project_root.as_ref().unwrap()
                };
                let target = base.join(&profile.prompt_target);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::write(&target, format!("用户原生内容{newline}")).unwrap();
            }

            let adapter = BuiltinAdapter::load(&expected.adapter_id).unwrap();
            let instances = adapter.detect(&ctx).unwrap();
            assert_eq!(
                instances.len(),
                expected.profiles.len(),
                "{} / {} profile 数量",
                suite.platform,
                expected.adapter_id
            );
            let (skill, skill_revision, skill_content) = skill_asset();
            let (prompt, prompt_revision) = prompt_asset(b"Always run the golden checks.\n");
            let (mcp, mcp_revision) = mcp_asset();

            for profile in &expected.profiles {
                let instance = instances
                    .iter()
                    .find(|instance| instance.profile == profile.profile)
                    .unwrap_or_else(|| {
                        panic!(
                            "{} / {} 缺少 profile {:?}",
                            suite.platform, expected.adapter_id, profile.profile
                        )
                    });
                let base = if profile.profile.is_none() {
                    &ctx.home
                } else {
                    ctx.project_root.as_ref().unwrap()
                };
                let scanned = adapter.scan(instance).unwrap();
                assert!(
                    scanned
                        .iter()
                        .any(|entry| entry.path == base.join(&profile.prompt_target)),
                    "{} / {} / {:?} 未扫描 prompt",
                    suite.platform,
                    expected.adapter_id,
                    profile.profile
                );

                let skill_output = adapter
                    .render_bundle(
                        &skill,
                        &skill_revision,
                        &skill_content,
                        instance,
                        &profile.scope,
                        &[],
                    )
                    .unwrap();
                let projected = &skill_output.projection.files[0];
                let ProjectionStrategy::DirectoryTree { relative_path } = &projected.strategy
                else {
                    panic!("Skill golden 投影不是 DirectoryTree");
                };
                assert_eq!(
                    projected.target_path.join(relative_path),
                    base.join(&profile.skill_target)
                );
                assert!(adapter
                    .plan_install(&skill_output.projection)
                    .unwrap()
                    .projection
                    .is_some());
                assert!(adapter
                    .plan_update(&skill_output.projection)
                    .unwrap()
                    .projection
                    .is_some());

                let prompt_output = adapter
                    .render(
                        &prompt,
                        &prompt_revision,
                        b"Always run the golden checks.\n",
                        instance,
                        &profile.scope,
                        &[],
                    )
                    .unwrap();
                assert_eq!(
                    prompt_output.projection.files[0].target_path,
                    base.join(&profile.prompt_target)
                );
                let removal = adapter
                    .plan_remove(&prompt, instance, &profile.scope)
                    .unwrap();
                assert_eq!(
                    removal.removals[0].target_path,
                    base.join(&profile.prompt_target)
                );

                match profile.mcp_mode.as_str() {
                    "structured_entry" => {
                        let output = adapter
                            .render(&mcp, &mcp_revision, b"{}", instance, &profile.scope, &[])
                            .unwrap();
                        assert_eq!(
                            output.projection.files[0].target_path,
                            base.join(profile.mcp_target.as_ref().unwrap())
                        );
                        assert!(matches!(
                            output.projection.files[0].strategy,
                            ProjectionStrategy::StructuredEntry { .. }
                        ));
                    }
                    "manual_required" => {
                        let output = adapter
                            .render(&mcp, &mcp_revision, b"{}", instance, &profile.scope, &[])
                            .unwrap();
                        assert!(output.projection.files.is_empty());
                        assert!(output
                            .projection
                            .compatibility_losses
                            .iter()
                            .any(|loss| loss.blocking));
                        let removal = adapter.plan_remove(&mcp, instance, &profile.scope).unwrap();
                        assert!(!removal.manual_required.is_empty());
                    }
                    "unsupported" => {
                        let error = adapter
                            .render(&mcp, &mcp_revision, b"{}", instance, &profile.scope, &[])
                            .unwrap_err();
                        assert_eq!(error.code, AdapterErrorCode::UnsupportedCapability);
                    }
                    other => panic!("未知 MCP golden mode：{other}"),
                }
            }
        }
    }

    #[test]
    fn every_builtin_manifest_is_valid_and_has_recovery_for_limitations() {
        let adapters = BuiltinAdapter::load_all().unwrap();
        assert_eq!(adapters.len(), 7);
        for adapter in adapters {
            assert!(!adapter.describe().surfaces.is_empty());
            assert!(adapter
                .describe()
                .limitations
                .iter()
                .all(|limitation| !limitation.recovery.trim().is_empty()));
        }
    }

    #[test]
    fn adapters_detect_from_isolated_home_and_project_fixtures() {
        for adapter in BuiltinAdapter::load_all().unwrap() {
            let temp = tempfile::tempdir().unwrap();
            let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
            let ctx = context(&root);
            fs::create_dir_all(&ctx.home).unwrap();
            fs::create_dir_all(ctx.project_root.as_ref().unwrap()).unwrap();
            let detection = adapter.describe().detection.first().unwrap();
            let marker = expand_root(&detection.path, &ctx).unwrap().unwrap();
            fs::create_dir_all(&marker).unwrap();
            let instances = adapter.detect(&ctx).unwrap();
            assert_eq!(instances.len(), 1, "{}", adapter.describe().adapter_id);
            assert!(!instances[0].surfaces.is_empty());
        }
    }

    #[test]
    fn codex_prompt_renders_to_managed_block_strategy() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let ctx = context(&root);
        fs::create_dir_all(ctx.home.join(".codex")).unwrap();
        fs::create_dir_all(ctx.project_root.as_ref().unwrap().join(".agents")).unwrap();
        let adapter = BuiltinAdapter::load("codex").unwrap();
        let instance = adapter
            .detect(&ctx)
            .unwrap()
            .into_iter()
            .find(|instance| instance.profile.as_deref() == Some("project"))
            .unwrap();
        let content = b"Always run tests.";
        let (asset, revision) = prompt_asset(content);
        let output = adapter
            .render(&asset, &revision, content, &instance, "project", &[])
            .unwrap();
        assert_eq!(output.objects[0].bytes, content);
        assert!(matches!(
            output.projection.files[0].strategy,
            ProjectionStrategy::ManagedBlock { .. }
        ));
        assert_eq!(
            output.projection.files[0].target_path,
            ctx.project_root.unwrap().join("AGENTS.md")
        );
    }

    #[test]
    fn pi_mcp_is_manual_required_instead_of_fake_local_config() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let ctx = context(&root);
        fs::create_dir_all(ctx.home.join(".pi/agent")).unwrap();
        fs::create_dir_all(ctx.project_root.as_ref().unwrap()).unwrap();
        let adapter = BuiltinAdapter::load("pi").unwrap();
        let instance = adapter.detect(&ctx).unwrap().remove(0);
        let identity = AssetIdentity::new("local", "fixture", ".", "example-mcp").unwrap();
        let asset = Asset::new(identity, AssetKind::McpServer);
        let content = b"{}";
        let hash = ContentHash::from_bytes(content);
        let revision = AssetRevision {
            id: "mcp-rev".to_owned(),
            raw_hash: hash.clone(),
            normalized_hash: normalized_hash(content),
            content_object: hash,
            source: Source {
                kind: SourceKind::LocalFolder,
                namespace: "fixture".to_owned(),
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
                server_name: "example-mcp".to_owned(),
                transport: McpTransport::Stdio {
                    command: "example".to_owned(),
                    args: Vec::new(),
                    env: BTreeMap::new(),
                },
                enabled: true,
                timeout_ms: None,
                oauth: None,
                allowed_tools: Vec::new(),
                denied_tools: Vec::new(),
            }),
            created_at_ms: 0,
            author: None,
            update_time_ms: None,
            platform_restrictions: Vec::new(),
        };
        let output = adapter
            .render(&asset, &revision, content, &instance, "global", &[])
            .unwrap();
        assert!(output.projection.files.is_empty());
        assert!(output.projection.compatibility_losses[0].blocking);
    }

    #[test]
    fn invalid_asset_name_cannot_escape_surface() {
        assert!(safe_target_name("../escape").is_err());
        assert!(safe_target_name("CON").is_err());
        assert!(safe_target_name("con.txt").is_err());
        assert!(safe_target_name("bad:name").is_err());
        assert!(safe_target_name(" leading-space").is_err());
        assert!(safe_target_name("valid-skill").is_ok());
    }

    #[test]
    fn scan_reports_skill_declared_name_mismatch_and_stable_logical_id() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let ctx = context(&root);
        fs::create_dir_all(ctx.home.join(".codex")).unwrap();
        let skill = ctx.home.join(".agents/skills/directory-name/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, b"---\nname: declared-name\n---\n# Skill\n").unwrap();

        let adapter = BuiltinAdapter::load("codex").unwrap();
        let instance = adapter
            .detect(&ctx)
            .unwrap()
            .into_iter()
            .find(|instance| instance.profile.is_none())
            .unwrap();
        let entry = adapter
            .scan(&instance)
            .unwrap()
            .into_iter()
            .find(|entry| entry.path == skill)
            .unwrap();

        assert_eq!(entry.logical_id.as_deref(), Some("skill:directory-name"));
        assert_eq!(entry.issue, Some(ObservationIssue::DeclaredNameMismatch));
    }

    #[test]
    fn hermes_external_skill_directories_are_scanned_but_not_install_targets() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let ctx = context(&root);
        let external = root.join("shared-skills");
        let skill = external.join("category/shared-demo/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(
            &skill,
            b"---\nname: shared-demo\ndescription: shared\n---\n# Shared\n",
        )
        .unwrap();
        fs::create_dir_all(ctx.home.join(".hermes")).unwrap();
        fs::write(
            ctx.home.join(".hermes/config.yaml"),
            format!("skills:\n  external_dirs:\n    - '{external}'\n"),
        )
        .unwrap();

        let adapter = BuiltinAdapter::load("hermes").unwrap();
        let instance = adapter
            .detect(&ctx)
            .unwrap()
            .into_iter()
            .find(|instance| instance.profile.is_none())
            .unwrap();
        let canonical = canonical_existing(&external).unwrap().unwrap();
        let configured = instance
            .surfaces
            .iter()
            .find(|surface| surface.root_path == canonical)
            .expect("应加载 Hermes external_dirs");
        assert!(!configured.writable);
        assert!(instance.managed_roots.contains(&canonical));
        let entry = adapter
            .scan(&instance)
            .unwrap()
            .into_iter()
            .find(|entry| entry.path.ends_with("shared-demo/SKILL.md"))
            .unwrap();
        assert_eq!(entry.logical_id.as_deref(), Some("skill:shared-demo"));
    }

    #[test]
    fn pi_settings_skill_globs_exclusions_and_direct_files_are_discovered() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let ctx = context(&root);
        let resources = root.join("pi-resources");
        let included = resources.join("included/SKILL.md");
        let ignored = resources.join("ignored/SKILL.md");
        let direct = resources.join("direct.md");
        fs::create_dir_all(included.parent().unwrap()).unwrap();
        fs::create_dir_all(ignored.parent().unwrap()).unwrap();
        fs::write(
            &included,
            b"---\nname: name-may-differ\ndescription: included\n---\n# Included\n",
        )
        .unwrap();
        fs::write(
            &ignored,
            b"---\nname: ignored\ndescription: ignored\n---\n# Ignored\n",
        )
        .unwrap();
        fs::write(
            &direct,
            b"---\nname: direct-skill\ndescription: direct\n---\n# Direct\n",
        )
        .unwrap();
        fs::create_dir_all(ctx.home.join(".pi/agent")).unwrap();
        let settings = serde_json::json!({
            "skills": [
                format!("{}/*", resources),
                format!("!{}/ignored", resources),
                direct,
            ]
        });
        fs::write(
            ctx.home.join(".pi/agent/settings.json"),
            serde_json::to_vec_pretty(&settings).unwrap(),
        )
        .unwrap();

        let adapter = BuiltinAdapter::load("pi").unwrap();
        let instance = adapter
            .detect(&ctx)
            .unwrap()
            .into_iter()
            .find(|instance| instance.profile.is_none())
            .unwrap();
        let scanned = adapter.scan(&instance).unwrap();
        let included = scanned
            .iter()
            .find(|entry| entry.path.ends_with("included/SKILL.md"))
            .unwrap();
        // Pi 官方允许 frontmatter 名称与目录名不同，不应误报冲突。
        assert_eq!(included.logical_id.as_deref(), Some("skill:included"));
        assert_eq!(included.issue, None);
        assert!(!scanned
            .iter()
            .any(|entry| entry.path.ends_with("ignored/SKILL.md")));
        assert!(scanned.iter().any(|entry| {
            entry.path.ends_with("direct.md")
                && entry.logical_id.as_deref() == Some("skill:direct-skill")
        }));
    }

    #[test]
    fn cold_scan_of_one_thousand_typical_skills_meets_budget() {
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let ctx = context(&root);
        fs::create_dir_all(ctx.home.join(".codex")).unwrap();
        for index in 0..1_000 {
            let skill = ctx
                .home
                .join(format!(".agents/skills/skill-{index:04}/SKILL.md"));
            fs::create_dir_all(skill.parent().unwrap()).unwrap();
            fs::write(
                skill,
                format!(
                    "---\nname: skill-{index:04}\ndescription: fixture\n---\n# Skill {index}\n"
                ),
            )
            .unwrap();
        }
        let adapter = BuiltinAdapter::load("codex").unwrap();
        let instance = adapter
            .detect(&ctx)
            .unwrap()
            .into_iter()
            .find(|instance| instance.profile.is_none())
            .unwrap();
        let mut samples = Vec::with_capacity(20);
        for _ in 0..20 {
            let started = Instant::now();
            let scanned = adapter.scan(&instance).unwrap();
            samples.push(started.elapsed());
            assert_eq!(
                scanned
                    .iter()
                    .filter(|entry| entry.path.file_name() == Some("SKILL.md"))
                    .count(),
                1_000
            );
        }
        samples.sort_unstable();
        let elapsed = samples[18];
        eprintln!("1,000 个典型 Skill 冷扫描 P95：{elapsed:?}");
        assert!(
            elapsed < Duration::from_secs(3),
            "1,000 个典型 Skill 冷扫描 P95 耗时 {elapsed:?}"
        );
    }

    #[test]
    fn multi_file_skill_bundle_projects_every_file_without_flattening() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let ctx = context(&root);
        fs::create_dir_all(ctx.home.join(".codex")).unwrap();
        fs::create_dir_all(ctx.project_root.as_ref().unwrap().join(".agents")).unwrap();
        let adapter = BuiltinAdapter::load("codex").unwrap();
        let instance = adapter
            .detect(&ctx)
            .unwrap()
            .into_iter()
            .find(|instance| instance.profile.as_deref() == Some("project"))
            .unwrap();
        let content = AssetContent {
            files: vec![
                rigdeck_adapter_sdk::AssetFileContent {
                    relative_path: "SKILL.md".into(),
                    bytes: b"---\nname: demo-skill\n---\n# Demo\n".to_vec(),
                    executable: false,
                },
                rigdeck_adapter_sdk::AssetFileContent {
                    relative_path: "scripts/run.sh".into(),
                    bytes: b"#!/bin/sh\necho safe\n".to_vec(),
                    executable: true,
                },
            ],
        };
        let identity = AssetIdentity::new("local", "fixture", ".", "demo-skill").unwrap();
        let asset = Asset::new(identity, AssetKind::Skill);
        let inventory = content.inventory_bytes().unwrap();
        let entry_hash = ContentHash::from_bytes(&content.files[0].bytes);
        let revision = AssetRevision {
            id: "skill-rev".to_owned(),
            raw_hash: content.raw_hash().unwrap(),
            normalized_hash: content.normalized_hash().unwrap(),
            content_object: entry_hash,
            source: Source {
                kind: SourceKind::LocalFolder,
                namespace: "fixture".to_owned(),
                locator: "fixture".to_owned(),
                revision: None,
            },
            license: None,
            audit: AuditResult {
                schema_version: 1,
                completed: true,
                findings: Vec::new(),
            },
            spec: AssetSpec::Skill(SkillSpec {
                entry_path: "SKILL.md".into(),
                inventory_object: ContentHash::from_bytes(&inventory),
                native_metadata: BTreeMap::new(),
            }),
            created_at_ms: 0,
            author: None,
            update_time_ms: None,
            platform_restrictions: Vec::new(),
        };
        let output = adapter
            .render_bundle(&asset, &revision, &content, &instance, "project", &[])
            .unwrap();
        assert_eq!(output.projection.files.len(), 2);
        assert!(output.projection.files.iter().any(|file| matches!(
            &file.strategy,
            ProjectionStrategy::DirectoryTree { relative_path }
                if relative_path == Utf8Path::new("scripts/run.sh")
        )));
    }

    #[test]
    fn windows_golden_fixtures_cover_all_seven_adapters() {
        run_golden_suite(include_str!("../fixtures/windows/golden.json"));
    }

    #[test]
    fn macos_golden_fixtures_cover_all_seven_adapters() {
        run_golden_suite(include_str!("../fixtures/macos/golden.json"));
    }

    #[test]
    fn rigdeck_manager_meta_skill_is_discoverable_on_all_seven_targets() {
        let targets: MetaTargets = serde_json::from_str(include_str!(
            "../../../packages/rigdeck-manager-skill/targets.json"
        ))
        .unwrap();
        assert_eq!(targets.schema_version, 1);
        assert_eq!(targets.skill, "rigdeck-manager");
        assert_eq!(targets.targets.len(), BUILTIN_ADAPTER_IDS.len());
        let (asset, revision, content) = skill_asset_from(
            &targets.skill,
            include_bytes!("../../../packages/rigdeck-manager-skill/SKILL.md").to_vec(),
        );
        for target in targets.targets {
            let temp = tempfile::tempdir().unwrap();
            let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
            let ctx = context(&root);
            fs::create_dir_all(&ctx.home).unwrap();
            fs::create_dir_all(ctx.project_root.as_ref().unwrap()).unwrap();
            let adapter = BuiltinAdapter::load(&target.adapter_id).unwrap();
            for detection in &adapter.describe().detection {
                if let Some(marker) = expand_root(&detection.path, &ctx).unwrap() {
                    fs::create_dir_all(marker).unwrap();
                }
            }
            let instances = adapter.detect(&ctx).unwrap();
            if let Some(expected) = target.global {
                let instance = instances
                    .iter()
                    .find(|instance| instance.profile.is_none())
                    .unwrap();
                let output = adapter
                    .render_bundle(&asset, &revision, &content, instance, "global", &[])
                    .unwrap();
                assert_eq!(
                    output.projection.files[0].target_path,
                    ctx.home.join(expected)
                );
            }
            let instance = instances
                .iter()
                .find(|instance| instance.profile.is_some())
                .unwrap();
            let output = adapter
                .render_bundle(&asset, &revision, &content, instance, "project", &[])
                .unwrap();
            assert_eq!(
                output.projection.files[0].target_path,
                ctx.project_root.as_ref().unwrap().join(target.project)
            );
        }
    }
}
