//! Agent 原生目录的跨平台 watcher 与批次去抖。

use std::{
    collections::BTreeSet,
    fs,
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

use camino::{Utf8Component, Utf8PathBuf};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rigdeck_adapter_sdk::{AdapterError, AdapterErrorCode, AdapterResult};

/// watcher 归一化后的变化种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchChangeKind {
    /// 创建。
    Create,
    /// 内容或 metadata 修改。
    Modify,
    /// 删除。
    Remove,
    /// 无法更精确分类的其他变化。
    Other,
}

/// 去抖后的一批本地路径变化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchBatch {
    /// 去重、排序后的 UTF-8 路径。
    pub paths: Vec<Utf8PathBuf>,
    /// 批次中出现的变化种类。
    pub kinds: BTreeSet<String>,
}

/// 持有操作系统 watcher 的运行时句柄。
///
/// `_watcher` 字段虽然不直接读取，但必须随服务存活；它的 `Drop` 会注销底层监听。
pub struct WatchService {
    _watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<Event>>,
    roots: Vec<Utf8PathBuf>,
    debounce: Duration,
}

impl std::fmt::Debug for WatchService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WatchService")
            .field("roots", &self.roots)
            .field("debounce", &self.debounce)
            .finish_non_exhaustive()
    }
}

impl WatchService {
    /// 对刷新后确认的根目录启动递归监听。
    pub fn start(roots: &[Utf8PathBuf], debounce: Duration) -> AdapterResult<Self> {
        if roots.is_empty() {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "watcher 至少需要一个根目录",
            ));
        }
        if debounce > Duration::from_secs(2) {
            return Err(AdapterError::new(
                AdapterErrorCode::ValidationFailed,
                "watcher debounce 不能超过 2 秒反馈预算",
            ));
        }
        let roots = validate_roots(roots)?;
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            // 接收端关闭表示服务已 Drop；回调不能 panic，也没有必要重试发送。
            let _ = sender.send(event);
        })
        .map_err(notify_error)?;
        for root in &roots {
            watcher
                .watch(root.as_std_path(), RecursiveMode::Recursive)
                .map_err(notify_error)?;
        }
        Ok(Self {
            _watcher: watcher,
            receiver,
            roots,
            debounce,
        })
    }

    /// 等待第一条事件，随后在 debounce 窗口内合并突发变化。
    ///
    /// 返回 `Ok(None)` 只是本次等待没有变化，不表示 watcher 失效。
    pub fn next_batch(&self, timeout: Duration) -> AdapterResult<Option<WatchBatch>> {
        let first = match self.receiver.recv_timeout(timeout) {
            Ok(event) => event.map_err(notify_error)?,
            Err(RecvTimeoutError::Timeout) => return Ok(None),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(AdapterError::new(
                    AdapterErrorCode::Io,
                    "watcher 事件通道已断开",
                ));
            }
        };
        let deadline = Instant::now() + self.debounce;
        let mut events = vec![first];
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self.receiver.recv_timeout(deadline - now) {
                Ok(event) => events.push(event.map_err(notify_error)?),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(AdapterError::new(
                        AdapterErrorCode::Io,
                        "watcher 事件通道已断开",
                    ));
                }
            }
        }
        normalize_events(&self.roots, events)
    }

    /// 当前监听的规范化根目录。
    pub fn roots(&self) -> &[Utf8PathBuf] {
        &self.roots
    }
}

fn validate_roots(roots: &[Utf8PathBuf]) -> AdapterResult<Vec<Utf8PathBuf>> {
    let mut output = BTreeSet::new();
    for root in roots {
        if !root.is_absolute()
            || root
                .components()
                .any(|part| matches!(part, Utf8Component::ParentDir))
        {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("watch root 必须是无 `..` 的绝对路径：{root}"),
            ));
        }
        let metadata = fs::symlink_metadata(root).map_err(|error| {
            AdapterError::new(
                AdapterErrorCode::Io,
                format!("无法读取 watch root {root}：{error}"),
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("watch root 必须是真实目录而不是 symlink：{root}"),
            ));
        }
        let canonical = fs::canonicalize(root).map_err(|error| {
            AdapterError::new(
                AdapterErrorCode::Io,
                format!("无法规范化 watch root {root}：{error}"),
            )
        })?;
        let canonical = Utf8PathBuf::from_path_buf(canonical).map_err(|path| {
            AdapterError::new(
                AdapterErrorCode::PathViolation,
                format!("watch root 不是 UTF-8：{}", path.display()),
            )
        })?;
        output.insert(strip_verbatim_prefix(canonical));
    }
    Ok(output.into_iter().collect())
}

fn normalize_events(
    roots: &[Utf8PathBuf],
    events: Vec<Event>,
) -> AdapterResult<Option<WatchBatch>> {
    let mut paths = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for event in events {
        kinds.insert(kind_name(&event.kind).to_owned());
        for path in event.paths {
            let path = Utf8PathBuf::from_path_buf(path).map_err(|path| {
                AdapterError::new(
                    AdapterErrorCode::PathViolation,
                    format!("watch 事件路径不是 UTF-8：{}", path.display()),
                )
            })?;
            let path = strip_verbatim_prefix(path);
            if roots.iter().any(|root| path.starts_with(root)) {
                paths.insert(path);
            }
        }
    }
    if paths.is_empty() {
        Ok(None)
    } else {
        Ok(Some(WatchBatch {
            paths: paths.into_iter().collect(),
            kinds,
        }))
    }
}

fn kind_name(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Create(_) => "create",
        EventKind::Modify(_) => "modify",
        EventKind::Remove(_) => "remove",
        _ => "other",
    }
}

fn notify_error(error: notify::Error) -> AdapterError {
    AdapterError::new(AdapterErrorCode::Io, format!("文件监听失败：{error}"))
}

/// 去掉 Windows `fs::canonicalize` 添加的 `\\?\` verbatim 前缀，
/// 使 canonicalize 后的路径与 `notify` 事件路径保持一致。
fn strip_verbatim_prefix(path: Utf8PathBuf) -> Utf8PathBuf {
    let s = path.as_str();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            return Utf8PathBuf::from(format!(r"\\{unc}"));
        }
        return Utf8PathBuf::from(rest);
    }
    path
}

#[cfg(test)]
mod tests {
    use std::{fs, thread};

    use super::*;

    /// 构造与 `WatchService` 内部 canonicalize+strip 后一致的 root。
    fn canonical_root(temp: &tempfile::TempDir) -> Utf8PathBuf {
        let canonical = Utf8PathBuf::from_path_buf(fs::canonicalize(temp.path()).unwrap()).unwrap();
        strip_verbatim_prefix(canonical)
    }

    #[test]
    fn watcher_reports_local_change_within_budget() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let watcher =
            WatchService::start(std::slice::from_ref(&root), Duration::from_millis(100)).unwrap();
        let canonical = canonical_root(&temp);
        let target = canonical.join("AGENTS.md");
        fs::write(&target, b"value").unwrap();
        let batch = watcher
            .next_batch(Duration::from_secs(2))
            .unwrap()
            .expect("应收到文件变化");
        assert!(batch.paths.iter().any(|path| path == &target));
    }

    #[test]
    fn burst_events_are_deduplicated() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let watcher =
            WatchService::start(std::slice::from_ref(&root), Duration::from_millis(150)).unwrap();
        let canonical = canonical_root(&temp);
        let target = canonical.join("value.txt");
        fs::write(&target, b"one").unwrap();
        thread::sleep(Duration::from_millis(20));
        fs::write(&target, b"two").unwrap();
        let batch = watcher
            .next_batch(Duration::from_secs(2))
            .unwrap()
            .expect("应收到文件变化");
        assert_eq!(
            batch.paths.iter().filter(|path| *path == &target).count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_watch_root_fails_closed() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let actual = Utf8PathBuf::from_path_buf(temp.path().join("actual")).unwrap();
        let linked = Utf8PathBuf::from_path_buf(temp.path().join("linked")).unwrap();
        fs::create_dir_all(&actual).unwrap();
        symlink(&actual, &linked).unwrap();
        let error = WatchService::start(&[linked], Duration::from_millis(50)).unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::PathViolation);
    }
}
