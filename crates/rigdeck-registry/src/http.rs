//! 带条件请求、限流状态和离线回退的 HTTP 读取层。

use std::{collections::BTreeMap, fs, io::Write, sync::Mutex, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use camino::{Utf8Path, Utf8PathBuf};
use reqwest::{
    header::{
        HeaderMap, HeaderValue, AUTHORIZATION, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH,
        LAST_MODIFIED, RETRY_AFTER,
    },
    StatusCode,
};
use rigdeck_security::SecretValue;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use url::Url;

use crate::{CacheState, RateLimitStatus, RegistryError, RegistryResult};

const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

/// 可持久化的 HTTP 缓存记录；不包含请求认证信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCacheRecord {
    /// 完整公开 URL。
    pub key: String,
    /// ETag。
    pub etag: Option<String>,
    /// Last-Modified。
    pub last_modified: Option<String>,
    /// 响应 body。
    pub body: Vec<u8>,
    /// 最近成功获取时间。
    pub fetched_at_ms: i64,
}

/// HTTP cache 抽象，生产可映射到 SQLite，测试可使用内存。
pub trait HttpCache: Send + Sync {
    /// 读取记录。
    fn get(&self, key: &str) -> RegistryResult<Option<HttpCacheRecord>>;
    /// 保存记录。
    fn put(&self, record: HttpCacheRecord) -> RegistryResult<()>;
}

/// 测试和无持久化预览使用的内存 cache。
#[derive(Debug, Default)]
pub struct MemoryHttpCache {
    records: Mutex<BTreeMap<String, HttpCacheRecord>>,
}

impl HttpCache for MemoryHttpCache {
    fn get(&self, key: &str) -> RegistryResult<Option<HttpCacheRecord>> {
        self.records
            .lock()
            .map_err(|_| RegistryError::Offline("HTTP cache 锁不可用".to_owned()))
            .map(|records| records.get(key).cloned())
    }

    fn put(&self, record: HttpCacheRecord) -> RegistryResult<()> {
        self.records
            .lock()
            .map_err(|_| RegistryError::Offline("HTTP cache 锁不可用".to_owned()))?
            .insert(record.key.clone(), record);
        Ok(())
    }
}

/// 持久化、原子写入的公开 HTTP metadata cache。
///
/// cache 文件名是 URL 的 Blake3 hash，body 使用 Base64 包在 JSON 中。目录不接受
/// symlink，读取时会再次核对 key，防止文件替换把另一来源响应冒充为当前 URL。
pub struct FileHttpCache {
    root: Utf8PathBuf,
    lock: Mutex<()>,
}

impl std::fmt::Debug for FileHttpCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileHttpCache")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl FileHttpCache {
    /// 创建或打开 cache 目录。
    pub fn open(root: impl Into<Utf8PathBuf>) -> RegistryResult<Self> {
        let root = root.into();
        if root.exists() {
            let metadata =
                fs::symlink_metadata(&root).map_err(|error| RegistryError::io(&root, error))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(RegistryError::Security(format!(
                    "HTTP cache 根必须是真实目录：{root}"
                )));
            }
        } else {
            fs::create_dir_all(&root).map_err(|error| RegistryError::io(&root, error))?;
        }
        Ok(Self {
            root,
            lock: Mutex::new(()),
        })
    }

    /// cache 根目录。
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    fn path_for(&self, key: &str) -> Utf8PathBuf {
        let hash = rigdeck_core::ContentHash::from_bytes(key.as_bytes());
        self.root.join(format!("{hash}.json"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct FileCacheEnvelope {
    schema_version: u32,
    key: String,
    etag: Option<String>,
    last_modified: Option<String>,
    body_base64: String,
    fetched_at_ms: i64,
}

impl HttpCache for FileHttpCache {
    fn get(&self, key: &str) -> RegistryResult<Option<HttpCacheRecord>> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| RegistryError::Offline("HTTP cache 锁不可用".to_owned()))?;
        let path = self.path_for(key);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(RegistryError::io(&path, error)),
        };
        if bytes.len() as u64 > MAX_RESPONSE_BYTES * 2 {
            return Err(RegistryError::Security(format!(
                "HTTP cache 文件异常过大：{path}"
            )));
        }
        let envelope: FileCacheEnvelope = serde_json::from_slice(&bytes)?;
        if envelope.schema_version != 1 || envelope.key != key {
            return Err(RegistryError::Security(format!(
                "HTTP cache key/schema 校验失败：{path}"
            )));
        }
        let body = BASE64
            .decode(envelope.body_base64)
            .map_err(|error| RegistryError::Security(format!("HTTP cache Base64 无效：{error}")))?;
        if body.len() as u64 > 64 * 1024 * 1024 {
            return Err(RegistryError::Security(
                "HTTP cache body 超过 64 MiB".to_owned(),
            ));
        }
        Ok(Some(HttpCacheRecord {
            key: envelope.key,
            etag: envelope.etag,
            last_modified: envelope.last_modified,
            body,
            fetched_at_ms: envelope.fetched_at_ms,
        }))
    }

    fn put(&self, record: HttpCacheRecord) -> RegistryResult<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| RegistryError::Offline("HTTP cache 锁不可用".to_owned()))?;
        if record.body.len() as u64 > 64 * 1024 * 1024 {
            return Err(RegistryError::Security(
                "HTTP cache body 超过 64 MiB".to_owned(),
            ));
        }
        let path = self.path_for(&record.key);
        let envelope = FileCacheEnvelope {
            schema_version: 1,
            key: record.key,
            etag: record.etag,
            last_modified: record.last_modified,
            body_base64: BASE64.encode(record.body),
            fetched_at_ms: record.fetched_at_ms,
        };
        let bytes = serde_json::to_vec(&envelope)?;
        let mut temporary = NamedTempFile::new_in(&self.root)
            .map_err(|error| RegistryError::io(&self.root, error))?;
        temporary
            .write_all(&bytes)
            .map_err(|error| RegistryError::io(temporary.path(), error))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| RegistryError::io(temporary.path(), error))?;
        temporary
            .persist(&path)
            .map_err(|error| RegistryError::io(&path, error.error))?;
        Ok(())
    }
}

/// 一次 HTTP 获取结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpFetch {
    /// 响应 body。
    pub body: Vec<u8>,
    /// 网络/缓存来源。
    pub cache_state: CacheState,
    /// 限流 headers。
    pub rate_limit: Option<RateLimitStatus>,
}

/// 只执行 GET 的安全 HTTP 客户端。
#[derive(Debug, Clone)]
pub struct ConditionalHttpClient {
    client: reqwest::Client,
}

impl ConditionalHttpClient {
    /// 创建带固定超时、有限重定向和 RigDeck User-Agent 的客户端。
    pub fn new() -> RegistryResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(concat!("RigDeck/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| RegistryError::Offline(error.to_string()))?;
        Ok(Self { client })
    }

    /// GET 并使用 ETag/Last-Modified 重验证。
    ///
    /// `bearer` 从系统钥匙串得到，只在本次请求构造期间借用，不写入 cache、URL 或
    /// 错误文本。网络失败时若有旧 cache，则返回 `stale_offline`。
    pub async fn get(
        &self,
        url: &Url,
        cache: &dyn HttpCache,
        bearer: Option<&SecretValue>,
    ) -> RegistryResult<HttpFetch> {
        self.get_limited(url, cache, bearer, MAX_RESPONSE_BYTES)
            .await
    }

    /// 与 [`Self::get`] 相同，但允许归档 provider 使用更高且仍有界的响应上限。
    pub async fn get_limited(
        &self,
        url: &Url,
        cache: &dyn HttpCache,
        bearer: Option<&SecretValue>,
        max_response_bytes: u64,
    ) -> RegistryResult<HttpFetch> {
        if max_response_bytes == 0 || max_response_bytes > 64 * 1024 * 1024 {
            return Err(RegistryError::Security(
                "HTTP 响应上限必须位于 1..=64 MiB".to_owned(),
            ));
        }
        validate_remote_url(url)?;
        let key = url.as_str().to_owned();
        let cached = cache.get(&key)?;
        let mut request = self.client.get(url.clone());
        if let Some(record) = &cached {
            if let Some(etag) = &record.etag {
                request = request.header(IF_NONE_MATCH, etag);
            }
            if let Some(last_modified) = &record.last_modified {
                request = request.header(IF_MODIFIED_SINCE, last_modified);
            }
        }
        if let Some(secret) = bearer {
            let mut bytes = b"Bearer ".to_vec();
            bytes.extend_from_slice(secret.expose());
            let value = HeaderValue::from_bytes(&bytes).map_err(|_| {
                RegistryError::AuthenticationRequired("Bearer token 含无效 header 字节".to_owned())
            })?;
            bytes.fill(0);
            request = request.header(AUTHORIZATION, value);
        }

        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                return cached.map_or_else(
                    || Err(RegistryError::Offline(error.to_string())),
                    |record| {
                        Ok(HttpFetch {
                            body: record.body,
                            cache_state: CacheState::StaleOffline,
                            rate_limit: None,
                        })
                    },
                );
            }
        };
        let status = response.status();
        let headers = response.headers().clone();
        let rate_limit = parse_rate_limit(&headers);
        if status == StatusCode::NOT_MODIFIED {
            let record = cached.ok_or_else(|| {
                RegistryError::BadResponse("服务端返回 304，但本地 cache 不存在".to_owned())
            })?;
            return Ok(HttpFetch {
                body: record.body,
                cache_state: CacheState::Revalidated,
                rate_limit,
            });
        }
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(RegistryError::AuthenticationRequired(format!(
                "HTTP {status}"
            )));
        }
        if status == StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound(url.to_string()));
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(RegistryError::RateLimited {
                retry_after_seconds: header_u64(&headers, RETRY_AFTER).unwrap_or(60),
                message: format!("HTTP {status}"),
            });
        }
        if !status.is_success() {
            return cached.map_or_else(
                || Err(RegistryError::BadResponse(format!("HTTP {status}"))),
                |record| {
                    Ok(HttpFetch {
                        body: record.body,
                        cache_state: CacheState::StaleOffline,
                        rate_limit,
                    })
                },
            );
        }
        if response
            .content_length()
            .is_some_and(|size| size > max_response_bytes)
        {
            return Err(RegistryError::Security(
                "远端响应 Content-Length 超过配置上限".to_owned(),
            ));
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| RegistryError::Offline(error.to_string()))?;
        if body.len() as u64 > max_response_bytes {
            return Err(RegistryError::Security(
                "远端响应实际大小超过配置上限".to_owned(),
            ));
        }
        let record = HttpCacheRecord {
            key,
            etag: header_string(&headers, ETAG),
            last_modified: header_string(&headers, LAST_MODIFIED),
            body: body.to_vec(),
            fetched_at_ms: now_ms(),
        };
        cache.put(record.clone())?;
        Ok(HttpFetch {
            body: record.body,
            cache_state: CacheState::Network,
            rate_limit,
        })
    }
}

impl Default for ConditionalHttpClient {
    fn default() -> Self {
        Self::new().expect("固定 HTTP client 配置应有效")
    }
}

/// 只允许 HTTPS；开发用明文 HTTP 仅限 loopback，防止凭据被发送到普通远端。
pub fn validate_remote_url(url: &Url) -> RegistryResult<()> {
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if (url.scheme() == "https" || (url.scheme() == "http" && loopback))
        && url.username().is_empty()
        && url.password().is_none()
    {
        return Ok(());
    }
    Err(RegistryError::InvalidSource(
        "远端 URL 必须是无 userinfo 的 HTTPS；HTTP 只允许 loopback".to_owned(),
    ))
}

fn parse_rate_limit(headers: &HeaderMap) -> Option<RateLimitStatus> {
    let result = RateLimitStatus {
        limit: header_name_u64(headers, "x-ratelimit-limit"),
        remaining: header_name_u64(headers, "x-ratelimit-remaining"),
        reset: header_name_u64(headers, "x-ratelimit-reset"),
    };
    (result.limit.is_some() || result.remaining.is_some() || result.reset.is_some())
        .then_some(result)
}

fn header_u64(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

fn header_name_u64(headers: &HeaderMap, name: &'static str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

fn header_string(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_owned)
}

fn now_ms() -> i64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_url_policy_rejects_plain_http_userinfo_and_non_web_schemes() {
        assert!(validate_remote_url(&Url::parse("https://example.com/api").unwrap()).is_ok());
        assert!(validate_remote_url(&Url::parse("http://localhost:8080/api").unwrap()).is_ok());
        assert!(validate_remote_url(&Url::parse("http://example.com/api").unwrap()).is_err());
        assert!(
            validate_remote_url(&Url::parse("https://token@example.com/api").unwrap()).is_err()
        );
        assert!(validate_remote_url(&Url::parse("file:///etc/passwd").unwrap()).is_err());
    }

    #[test]
    fn memory_cache_round_trip() {
        let cache = MemoryHttpCache::default();
        let record = HttpCacheRecord {
            key: "https://example.com".to_owned(),
            etag: Some("abc".to_owned()),
            last_modified: None,
            body: b"value".to_vec(),
            fetched_at_ms: 1,
        };
        cache.put(record.clone()).unwrap();
        assert_eq!(cache.get(&record.key).unwrap(), Some(record));
    }

    #[test]
    fn file_cache_round_trip_and_key_tampering_detection() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let cache = FileHttpCache::open(root).unwrap();
        let record = HttpCacheRecord {
            key: "https://example.com/data".to_owned(),
            etag: Some("etag-1".to_owned()),
            last_modified: None,
            body: b"public metadata".to_vec(),
            fetched_at_ms: 1,
        };
        cache.put(record.clone()).unwrap();
        assert_eq!(cache.get(&record.key).unwrap(), Some(record.clone()));

        let path = cache.path_for(&record.key);
        let mut envelope: FileCacheEnvelope =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        envelope.key = "https://attacker.invalid".to_owned();
        fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(matches!(
            cache.get(&record.key),
            Err(RegistryError::Security(_))
        ));
    }
}
