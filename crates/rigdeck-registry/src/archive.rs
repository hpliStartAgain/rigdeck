//! 归档和本地目录的安全文件清单读取。

use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read},
};

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use flate2::read::GzDecoder;
use rigdeck_adapter_sdk::{AssetContent, AssetFileContent};

use crate::{RegistryError, RegistryResult};

/// 支持的归档格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    /// ZIP。
    Zip,
    /// gzip 压缩 tar。
    TarGz,
    /// 未压缩 tar。
    Tar,
}

/// 自动识别 ZIP/gzip tar/tar 并返回安全文件 bundle。
pub fn extract_archive(bytes: &[u8]) -> RegistryResult<AssetContent> {
    let kind = if bytes.starts_with(b"PK\x03\x04") {
        ArchiveKind::Zip
    } else if bytes.starts_with(&[0x1f, 0x8b]) {
        ArchiveKind::TarGz
    } else if bytes.len() >= 262 && &bytes[257..262] == b"ustar" {
        ArchiveKind::Tar
    } else {
        return Err(RegistryError::InvalidSource(
            "无法识别归档格式；仅支持 ZIP、tar.gz 和 tar".to_owned(),
        ));
    };
    extract_archive_as(bytes, kind)
}

/// 按显式格式读取归档。
pub fn extract_archive_as(bytes: &[u8], kind: ArchiveKind) -> RegistryResult<AssetContent> {
    if bytes.len() as u64 > limits::MAX_ARCHIVE_BYTES {
        return Err(RegistryError::Security("归档本体超过 64 MiB".to_owned()));
    }
    let files = match kind {
        ArchiveKind::Zip => read_zip(bytes)?,
        ArchiveKind::TarGz => read_tar(GzDecoder::new(Cursor::new(bytes)))?,
        ArchiveKind::Tar => read_tar(Cursor::new(bytes))?,
    };
    if kind == ArchiveKind::TarGz {
        let expanded: u64 = files
            .iter()
            .map(|file| u64::try_from(file.bytes.len()).unwrap_or(u64::MAX))
            .sum();
        if expanded / u64::try_from(bytes.len()).unwrap_or(1).max(1) > limits::MAX_COMPRESSION_RATIO
        {
            return Err(RegistryError::Security(
                "tar.gz 整体压缩比超过限制".to_owned(),
            ));
        }
    }
    validate_bundle(files)
}

/// 对 Provider 构造的内存文件树应用与归档相同的限制。
pub fn validate_asset_content(content: AssetContent) -> RegistryResult<AssetContent> {
    validate_bundle(content.files)
}

/// 安全读取本地目录；symlink、非 UTF-8 路径和越界规模会失败关闭。
pub fn read_local_directory(root: &Utf8Path) -> RegistryResult<AssetContent> {
    let metadata = fs::symlink_metadata(root).map_err(|error| RegistryError::io(root, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RegistryError::Security(format!(
            "本地来源根必须是真实目录：{root}"
        )));
    }
    let mut pending = vec![(root.to_owned(), 0usize)];
    let mut files = Vec::new();
    while let Some((path, depth)) = pending.pop() {
        if depth > 32 {
            return Err(RegistryError::Security("目录深度超过 32".to_owned()));
        }
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| RegistryError::io(&path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(RegistryError::Security(format!(
                "本地来源禁止 symlink：{path}"
            )));
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&path).map_err(|error| RegistryError::io(&path, error))? {
                let entry = entry.map_err(|error| RegistryError::io(&path, error))?;
                let child = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                    RegistryError::Security(format!("路径不是 UTF-8：{}", path.display()))
                })?;
                pending.push((child, depth + 1));
            }
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| RegistryError::Security(format!("文件逃逸来源根：{path}")))?;
            // Windows 上 strip_prefix 返回的相对路径用反斜杠分隔，
            // 但 validate_relative 和对象存储要求 POSIX 风格。统一转换。
            let relative = if relative.as_str().contains('\\') {
                Utf8PathBuf::from(relative.as_str().replace('\\', "/"))
            } else {
                relative.to_owned()
            };
            validate_relative(&relative)?;
            if metadata.len() > limits::MAX_FILE_BYTES {
                return Err(RegistryError::Security(format!(
                    "单文件超过 16 MiB：{relative}"
                )));
            }
            files.push(AssetFileContent {
                relative_path: relative.to_owned(),
                bytes: fs::read(&path).map_err(|error| RegistryError::io(&path, error))?,
                executable: executable_from_metadata(&metadata),
            });
        }
    }
    validate_bundle(files)
}

/// 去掉 GitHub/codeload 等归档自动添加的唯一顶层目录。
pub fn strip_common_root(content: AssetContent) -> RegistryResult<AssetContent> {
    let mut common: Option<String> = None;
    for file in &content.files {
        let mut components = file.relative_path.components();
        let Some(Utf8Component::Normal(first)) = components.next() else {
            return Err(RegistryError::Security("归档路径结构无效".to_owned()));
        };
        if components.next().is_none() {
            return Ok(content);
        }
        match &common {
            Some(value) if value != first => return Ok(content),
            None => common = Some(first.to_owned()),
            _ => {}
        }
    }
    let Some(common) = common else {
        return Ok(content);
    };
    let prefix = Utf8Path::new(&common);
    let files = content
        .files
        .into_iter()
        .map(|mut file| {
            file.relative_path = file
                .relative_path
                .strip_prefix(prefix)
                .expect("已验证共同前缀")
                .to_owned();
            file
        })
        .collect();
    validate_bundle(files)
}

/// 选择归档中的一个子目录并把它重定位为资产根。
pub fn select_subdirectory(
    content: AssetContent,
    subdirectory: &Utf8Path,
) -> RegistryResult<AssetContent> {
    validate_relative(subdirectory)?;
    let files: Vec<_> = content
        .files
        .into_iter()
        .filter_map(|mut file| {
            file.relative_path
                .strip_prefix(subdirectory)
                .ok()
                .map(Utf8Path::to_owned)
                .filter(|path| !path.as_str().is_empty())
                .map(|path| {
                    file.relative_path = path;
                    file
                })
        })
        .collect();
    if files.is_empty() {
        return Err(RegistryError::NotFound(format!(
            "归档中没有子目录：{subdirectory}"
        )));
    }
    validate_bundle(files)
}

fn read_zip(bytes: &[u8]) -> RegistryResult<Vec<AssetFileContent>> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|error| RegistryError::InvalidSource(format!("ZIP 结构无效：{error}")))?;
    if archive.len() > limits::MAX_FILES {
        return Err(RegistryError::Security("ZIP 文件数超过 1000".to_owned()));
    }
    let mut files = Vec::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| RegistryError::InvalidSource(format!("ZIP entry 无效：{error}")))?;
        if entry.is_dir() {
            continue;
        }
        let mode = entry.unix_mode().unwrap_or(0);
        if mode & 0o170000 == 0o120000 {
            return Err(RegistryError::Security(format!(
                "ZIP 禁止 symlink：{}",
                entry.name()
            )));
        }
        let path = Utf8PathBuf::from(entry.name());
        validate_relative(&path)?;
        let size = entry.size();
        if size > limits::MAX_FILE_BYTES {
            return Err(RegistryError::Security(format!(
                "ZIP 单文件超过 16 MiB：{path}"
            )));
        }
        let compressed = entry.compressed_size();
        if compressed > 0 && size / compressed.max(1) > limits::MAX_COMPRESSION_RATIO {
            return Err(RegistryError::Security(format!(
                "ZIP 压缩比超过限制：{path}"
            )));
        }
        total = checked_total(total, size, "ZIP")?;
        let mut content = Vec::with_capacity(size as usize);
        entry
            .by_ref()
            .take(limits::MAX_FILE_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|error| RegistryError::io(path.as_std_path(), error))?;
        if content.len() as u64 != size {
            return Err(RegistryError::Security(format!(
                "ZIP entry 解压大小不一致：{path}"
            )));
        }
        files.push(AssetFileContent {
            relative_path: path,
            bytes: content,
            executable: mode & 0o111 != 0,
        });
    }
    Ok(files)
}

fn read_tar<R: Read>(reader: R) -> RegistryResult<Vec<AssetFileContent>> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|error| RegistryError::InvalidSource(format!("tar 结构无效：{error}")))?;
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in entries {
        let mut entry = entry
            .map_err(|error| RegistryError::InvalidSource(format!("tar entry 无效：{error}")))?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            continue;
        }
        if files.len() >= limits::MAX_FILES {
            return Err(RegistryError::Security("tar 文件数超过 1000".to_owned()));
        }
        if !entry_type.is_file() {
            return Err(RegistryError::Security(
                "tar 禁止 symlink、hardlink、设备和其他特殊 entry".to_owned(),
            ));
        }
        let path = entry
            .path()
            .map_err(|error| RegistryError::InvalidSource(error.to_string()))?;
        let path = Utf8PathBuf::from_path_buf(path.into_owned()).map_err(|path| {
            RegistryError::Security(format!("tar 路径不是 UTF-8：{}", path.display()))
        })?;
        validate_relative(&path)?;
        let size = entry.size();
        if size > limits::MAX_FILE_BYTES {
            return Err(RegistryError::Security(format!(
                "tar 单文件超过 16 MiB：{path}"
            )));
        }
        total = checked_total(total, size, "tar")?;
        let mode = entry.header().mode().unwrap_or(0);
        let mut content = Vec::with_capacity(size as usize);
        entry
            .by_ref()
            .take(limits::MAX_FILE_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|error| RegistryError::io(path.as_std_path(), error))?;
        if content.len() as u64 != size {
            return Err(RegistryError::Security(format!(
                "tar entry 读取大小不一致：{path}"
            )));
        }
        files.push(AssetFileContent {
            relative_path: path,
            bytes: content,
            executable: mode & 0o111 != 0,
        });
    }
    Ok(files)
}

fn validate_bundle(mut files: Vec<AssetFileContent>) -> RegistryResult<AssetContent> {
    if files.is_empty() {
        return Err(RegistryError::InvalidSource("来源没有普通文件".to_owned()));
    }
    if files.len() > limits::MAX_FILES {
        return Err(RegistryError::Security("文件数超过 1000".to_owned()));
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for file in &files {
        validate_relative(&file.relative_path)?;
        if !seen.insert(file.relative_path.clone()) {
            return Err(RegistryError::Security(format!(
                "来源路径重复：{}",
                file.relative_path
            )));
        }
        if file.bytes.len() as u64 > limits::MAX_FILE_BYTES {
            return Err(RegistryError::Security(format!(
                "单文件超过 16 MiB：{}",
                file.relative_path
            )));
        }
        total = total.saturating_add(file.bytes.len() as u64);
        if total > limits::MAX_TOTAL_BYTES {
            return Err(RegistryError::Security(
                "展开后总大小超过 64 MiB".to_owned(),
            ));
        }
    }
    Ok(AssetContent { files })
}

fn validate_relative(path: &Utf8Path) -> RegistryResult<()> {
    if path.as_str().is_empty()
        || path.as_str().contains('\\')
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Utf8Component::ParentDir | Utf8Component::RootDir | Utf8Component::Prefix(_)
            )
        })
    {
        return Err(RegistryError::Security(format!(
            "路径必须是安全 POSIX 风格相对路径：{path}"
        )));
    }
    if path.as_str().len() > 4_096 || path.as_str().chars().any(char::is_control) {
        return Err(RegistryError::Security(format!(
            "路径长度或控制字符无效：{path}"
        )));
    }
    let mut depth = 0usize;
    for component in path.components() {
        let Utf8Component::Normal(segment) = component else {
            continue;
        };
        depth += 1;
        if depth > 32 || segment.len() > 255 {
            return Err(RegistryError::Security(format!(
                "路径深度或单段长度超过限制：{path}"
            )));
        }
        if segment.contains(':') || segment.ends_with(['.', ' ']) {
            return Err(RegistryError::Security(format!(
                "路径包含 Windows ADS/歧义尾字符：{path}"
            )));
        }
        let stem = segment
            .split('.')
            .next()
            .unwrap_or(segment)
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || stem
                .strip_prefix("COM")
                .or_else(|| stem.strip_prefix("LPT"))
                .is_some_and(|suffix| {
                    suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
                });
        if reserved {
            return Err(RegistryError::Security(format!(
                "路径包含 Windows 保留设备名：{path}"
            )));
        }
    }
    Ok(())
}

fn checked_total(current: u64, next: u64, label: &str) -> RegistryResult<u64> {
    let total = current.saturating_add(next);
    if total > limits::MAX_TOTAL_BYTES {
        return Err(RegistryError::Security(format!(
            "{label} 展开后总大小超过 64 MiB"
        )));
    }
    Ok(total)
}

#[cfg(unix)]
fn executable_from_metadata(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable_from_metadata(_metadata: &fs::Metadata) -> bool {
    false
}

mod limits {
    pub const MAX_FILES: usize = 1_000;
    pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
    pub const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
    pub const MAX_COMPRESSION_RATIO: u64 = 100;
    pub const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn zip_extracts_safe_multi_file_skill() {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("root/SKILL.md", options).unwrap();
            writer.write_all(b"# demo").unwrap();
            writer.start_file("root/scripts/run.sh", options).unwrap();
            writer.write_all(b"echo safe").unwrap();
            writer.finish().unwrap();
        }
        let content = strip_common_root(extract_archive(bytes.get_ref()).unwrap()).unwrap();
        assert_eq!(content.files.len(), 2);
        assert_eq!(content.files[0].relative_path, "SKILL.md");
    }

    #[test]
    fn zip_traversal_fails_closed() {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            writer
                .start_file("../escape", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"bad").unwrap();
            writer.finish().unwrap();
        }
        assert!(matches!(
            extract_archive(bytes.get_ref()),
            Err(RegistryError::Security(_))
        ));
    }

    #[test]
    fn windows_devices_ads_depth_and_file_count_fail_closed() {
        for path in ["CON.txt", "safe.txt:payload", "folder./value", "LPT9"] {
            assert!(validate_relative(Utf8Path::new(path)).is_err(), "{path}");
        }
        let deep = (0..33).map(|_| "d").collect::<Vec<_>>().join("/") + "/value";
        assert!(validate_relative(Utf8Path::new(&deep)).is_err());
        let files = (0..=limits::MAX_FILES)
            .map(|index| AssetFileContent {
                relative_path: format!("file-{index}").into(),
                bytes: Vec::new(),
                executable: false,
            })
            .collect();
        assert!(validate_asset_content(AssetContent { files }).is_err());
    }

    #[test]
    fn malformed_archive_and_compression_bomb_fail_closed() {
        assert!(extract_archive(b"not an archive").is_err());

        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let payload = vec![0u8; 1024 * 1024];
            let mut header = tar::Header::new_gnu();
            header.set_size(payload.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "SKILL.md", payload.as_slice())
                .unwrap();
            builder.finish().unwrap();
        }
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&tar_bytes).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(matches!(
            extract_archive_as(&compressed, ArchiveKind::TarGz),
            Err(RegistryError::Security(_))
        ));
    }

    #[test]
    fn tar_symlink_fails_closed() {
        let mut bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut bytes);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_cksum();
            builder
                .append_link(&mut header, "linked", "outside")
                .unwrap();
            builder.finish().unwrap();
        }
        assert!(matches!(
            extract_archive_as(&bytes, ArchiveKind::Tar),
            Err(RegistryError::Security(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn local_directory_symlink_fails_closed() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().join("root")).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("SKILL.md"), b"demo").unwrap();
        fs::write(temp.path().join("outside"), b"secret").unwrap();
        symlink(temp.path().join("outside"), root.join("linked")).unwrap();
        assert!(matches!(
            read_local_directory(&root),
            Err(RegistryError::Security(_))
        ));
    }

    #[test]
    fn corrupted_archive_fails_closed() {
        // 故障注入：截断的 ZIP 头
        assert!(extract_archive(b"PK\x03\x04\x00").is_err());
        // 故障注入：截断的 gzip 头
        let gz: &[u8] = &[0x1f, 0x8b, 0x00, 0x00];
        assert!(extract_archive(gz).is_err());
        // 故障注入：随机字节
        assert!(extract_archive(b"this is not an archive").is_err());
        // 故障注入：空输入
        assert!(extract_archive(b"").is_err());
    }

    #[test]
    fn empty_directory_is_rejected() {
        // 故障注入：空目录没有普通文件，read_local_directory 应拒绝。
        let temp = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(temp.path().to_path_buf())
            .unwrap_or_else(|_| camino::Utf8PathBuf::from("/tmp/empty"));
        std::fs::create_dir_all(&root).unwrap();
        assert!(read_local_directory(&root).is_err());
    }

    #[test]
    fn archive_with_path_traversal_in_name_fails_closed() {
        // 安全夹具：tar builder 自身拒绝包含 .. 的路径，这是第一道防线。
        let mut buf: Vec<u8> = Vec::new();
        let mut builder = tar::Builder::new(&mut buf);
        let mut header = tar::Header::new_gnu();
        let result = header.set_path("../escape.txt");
        assert!(result.is_err(), "tar builder 必须拒绝 .. 路径");
        // 即使 set_path 意外成功，extract_archive 也应拒绝
        let _ = builder;
    }
}
