//! Windows/macOS/Linux 开发环境的应用路径。

use std::env;

use camino::Utf8PathBuf;

use crate::{ServiceError, ServiceResult};

/// RigDeck 所有私有状态路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// 应用数据根。
    pub data_root: Utf8PathBuf,
    /// SQLite 数据库。
    pub database: Utf8PathBuf,
    /// 加密内容对象库。
    pub object_store: Utf8PathBuf,
    /// 公开 HTTP metadata cache。
    pub http_cache: Utf8PathBuf,
    /// 一致性备份。
    pub backups: Utf8PathBuf,
    /// 第三方 Adapter 包。
    pub adapters: Utf8PathBuf,
}

impl AppPaths {
    /// 在给定根下构造全部路径，主要供测试和便携模式使用。
    pub fn for_root(root: impl Into<Utf8PathBuf>) -> Self {
        let data_root = root.into();
        Self {
            database: data_root.join("rigdeck.sqlite3"),
            object_store: data_root.join("store"),
            http_cache: data_root.join("cache/http"),
            backups: data_root.join("backups"),
            adapters: data_root.join("adapters"),
            data_root,
        }
    }

    /// 按当前平台发现默认应用数据目录。
    pub fn discover() -> ServiceResult<Self> {
        let root = if cfg!(target_os = "windows") {
            env_path("LOCALAPPDATA")?.join("RigDeck")
        } else if cfg!(target_os = "macos") {
            home()?.join("Library/Application Support/RigDeck")
        } else if let Some(value) = env::var_os("XDG_DATA_HOME") {
            utf8_env("XDG_DATA_HOME", value)?.join("rigdeck")
        } else {
            home()?.join(".local/share/rigdeck")
        };
        Ok(Self::for_root(root))
    }
}

/// 当前用户 HOME，供 Agent detection 使用。
pub fn home() -> ServiceResult<Utf8PathBuf> {
    if cfg!(target_os = "windows") {
        env_path("USERPROFILE")
    } else {
        env_path("HOME")
    }
}

fn env_path(name: &str) -> ServiceResult<Utf8PathBuf> {
    let value = env::var_os(name)
        .ok_or_else(|| ServiceError::InvalidInput(format!("环境变量 {name} 不存在")))?;
    utf8_env(name, value)
}

fn utf8_env(name: &str, value: std::ffi::OsString) -> ServiceResult<Utf8PathBuf> {
    value
        .into_string()
        .map(Utf8PathBuf::from)
        .map_err(|_| ServiceError::InvalidInput(format!("环境变量 {name} 不是 UTF-8 路径")))
}
