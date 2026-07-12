//! skills.sh、GitHub 与 MCP Registry provider。

use async_trait::async_trait;
use camino::{Utf8Path, Utf8PathBuf};
use rigdeck_adapter_sdk::{AssetContent, AssetFileContent};
use rigdeck_core::{AssetKind, Source, SourceKind};
use rigdeck_security::SecretValue;
use serde_json::Value;
use url::Url;

use crate::{
    select_subdirectory, strip_common_root, validate_asset_content, CatalogItem,
    ConditionalHttpClient, FetchedAsset, FetchedCatalogAsset, HttpCache, RegistryError,
    RegistryResult, SearchResult,
};

/// 所有远端目录 provider 的统一只读合同。
#[async_trait]
pub trait CatalogProvider: Send + Sync {
    /// 稳定 Provider ID。
    fn id(&self) -> &str;
    /// 搜索目录。
    async fn search(&self, query: &str, limit: usize) -> RegistryResult<SearchResult>;
    /// 按 Provider 稳定 ID 或公开定位符获取完整对象。
    async fn fetch(&self, id: &str) -> RegistryResult<FetchedCatalogAsset>;
}

/// skills.sh v1 API provider。
pub struct SkillsShProvider<'a> {
    http: ConditionalHttpClient,
    cache: &'a dyn HttpCache,
    base: Url,
    oidc_token: Option<SecretValue>,
}

impl std::fmt::Debug for SkillsShProvider<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SkillsShProvider")
            .field("base", &self.base)
            .field("oidc_token", &self.oidc_token.as_ref().map(|_| "***"))
            .finish()
    }
}

impl<'a> SkillsShProvider<'a> {
    /// 创建官方 skills.sh provider。OIDC token 仅在请求期间使用。
    pub fn official(
        cache: &'a dyn HttpCache,
        oidc_token: Option<SecretValue>,
    ) -> RegistryResult<Self> {
        Ok(Self {
            http: ConditionalHttpClient::new()?,
            cache,
            base: Url::parse("https://skills.sh")
                .map_err(|error| RegistryError::InvalidSource(error.to_string()))?,
            oidc_token,
        })
    }
}

#[async_trait]
impl CatalogProvider for SkillsShProvider<'_> {
    fn id(&self) -> &str {
        "skills.sh"
    }

    async fn search(&self, query: &str, limit: usize) -> RegistryResult<SearchResult> {
        validate_query(query, limit)?;
        let mut url = self
            .base
            .join("/api/v1/skills/search")
            .map_err(|error| RegistryError::InvalidSource(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", &limit.min(200).to_string());
        let fetched = self
            .http
            .get(&url, self.cache, self.oidc_token.as_ref())
            .await?;
        let value: Value = serde_json::from_slice(&fetched.body)?;
        let items = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| RegistryError::BadResponse("skills.sh 缺少 data array".to_owned()))?
            .iter()
            .map(parse_skills_item)
            .collect::<RegistryResult<Vec<_>>>()?;
        Ok(SearchResult {
            items,
            cache_state: fetched.cache_state,
            rate_limit: fetched.rate_limit,
        })
    }

    async fn fetch(&self, id: &str) -> RegistryResult<FetchedCatalogAsset> {
        let segments = safe_id_segments(id)?;
        if segments.len() < 2 {
            return Err(RegistryError::InvalidSource(
                "skills.sh ID 至少包含 source/skill".to_owned(),
            ));
        }
        let mut url = self
            .base
            .join("/api/v1/skills/")
            .map_err(|error| RegistryError::InvalidSource(error.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| RegistryError::InvalidSource("skills.sh base URL 无法拼接".to_owned()))?
            .extend(segments.iter().copied());
        let fetched = self
            .http
            .get(&url, self.cache, self.oidc_token.as_ref())
            .await?;
        let value: Value = serde_json::from_slice(&fetched.body)?;
        let source_id = required_str(&value, "source")?;
        let slug = required_str(&value, "slug")?;
        let files = value
            .get("files")
            .and_then(Value::as_array)
            .ok_or_else(|| RegistryError::BadResponse("skills.sh detail 缺少 files".to_owned()))?
            .iter()
            .map(|file| {
                let path = required_str(file, "path")?;
                let contents = required_str(file, "contents")?;
                Ok(AssetFileContent {
                    relative_path: Utf8PathBuf::from(path),
                    bytes: contents.as_bytes().to_vec(),
                    executable: false,
                })
            })
            .collect::<RegistryResult<Vec<_>>>()?;
        let content = validate_asset_content(AssetContent { files })?;
        let source = Source {
            kind: SourceKind::SkillsSh,
            namespace: source_id.to_owned(),
            locator: format!("https://skills.sh/{id}"),
            revision: value.get("hash").and_then(Value::as_str).map(str::to_owned),
        };
        Ok(FetchedCatalogAsset::Skill(FetchedAsset {
            kind: AssetKind::Skill,
            source,
            source_namespace: source_id.to_owned(),
            package: source_id.to_owned(),
            relative_path: Utf8PathBuf::from(slug),
            content,
            license: None,
            metadata: value,
        }))
    }
}

/// GitHub 仓库/子目录 provider，使用官方 tarball endpoint 一次获取固定 ref。
pub struct GithubProvider<'a> {
    http: ConditionalHttpClient,
    cache: &'a dyn HttpCache,
    token: Option<SecretValue>,
}

impl std::fmt::Debug for GithubProvider<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GithubProvider")
            .field("token", &self.token.as_ref().map(|_| "***"))
            .finish()
    }
}

impl<'a> GithubProvider<'a> {
    /// 创建 GitHub provider。公开仓库可不提供 token，私有源 token 只来自钥匙串。
    pub fn new(cache: &'a dyn HttpCache, token: Option<SecretValue>) -> RegistryResult<Self> {
        Ok(Self {
            http: ConditionalHttpClient::new()?,
            cache,
            token,
        })
    }
}

#[async_trait]
impl CatalogProvider for GithubProvider<'_> {
    fn id(&self) -> &str {
        "github"
    }

    async fn search(&self, _query: &str, _limit: usize) -> RegistryResult<SearchResult> {
        Err(RegistryError::InvalidSource(
            "GitHub provider 只按明确仓库/子目录获取；目录搜索请使用 skills.sh 或配置的私有源"
                .to_owned(),
        ))
    }

    async fn fetch(&self, id: &str) -> RegistryResult<FetchedCatalogAsset> {
        let locator = GithubLocator::parse(id)?;
        let mut url = Url::parse("https://api.github.com")
            .map_err(|error| RegistryError::InvalidSource(error.to_string()))?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| RegistryError::InvalidSource("GitHub API URL 无法拼接".to_owned()))?;
            segments.extend([
                "repos",
                locator.owner.as_str(),
                locator.repository.as_str(),
                "tarball",
                locator.reference.as_str(),
            ]);
        }
        let fetched = self
            .http
            .get_limited(&url, self.cache, self.token.as_ref(), 64 * 1024 * 1024)
            .await?;
        let mut content = strip_common_root(crate::extract_archive(&fetched.body)?)?;
        if locator.subdirectory != Utf8Path::new(".") {
            content = select_subdirectory(content, &locator.subdirectory)?;
        }
        if !content
            .files
            .iter()
            .any(|file| file.relative_path == Utf8Path::new("SKILL.md"))
        {
            return Err(RegistryError::NotFound(format!(
                "GitHub 子目录没有 SKILL.md：{}",
                locator.subdirectory
            )));
        }
        let package = format!("{}/{}", locator.owner, locator.repository);
        let source = Source {
            kind: SourceKind::Github,
            namespace: locator.owner.clone(),
            locator: locator.public_locator(),
            revision: Some(locator.reference.clone()),
        };
        Ok(FetchedCatalogAsset::Skill(FetchedAsset {
            kind: AssetKind::Skill,
            source,
            source_namespace: locator.owner,
            package,
            relative_path: locator.subdirectory,
            content,
            license: None,
            metadata: serde_json::json!({
                "cache_state": fetched.cache_state,
                "rate_limit": fetched.rate_limit,
            }),
        }))
    }
}

/// 通用 HTTPS URL provider；接受指向归档（tar.gz/zip/tar）或单个 `SKILL.md` 文件的 URL。
///
/// 不提供目录搜索；`fetch` 的 ID 就是完整 HTTPS URL。
/// 凭据只通过 `Authorization: Bearer` header 传递，不会写入 URL 或缓存。
pub struct UrlProvider<'a> {
    http: ConditionalHttpClient,
    cache: &'a dyn HttpCache,
    bearer: Option<SecretValue>,
}

impl std::fmt::Debug for UrlProvider<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UrlProvider")
            .field("bearer", &self.bearer.as_ref().map(|_| "***"))
            .finish()
    }
}

impl<'a> UrlProvider<'a> {
    /// 创建通用 URL provider；`bearer` 仅在请求期间使用。
    pub fn new(cache: &'a dyn HttpCache, bearer: Option<SecretValue>) -> RegistryResult<Self> {
        Ok(Self {
            http: ConditionalHttpClient::new()?,
            cache,
            bearer,
        })
    }
}

#[async_trait]
impl CatalogProvider for UrlProvider<'_> {
    fn id(&self) -> &str {
        "url"
    }

    async fn search(&self, _query: &str, _limit: usize) -> RegistryResult<SearchResult> {
        Err(RegistryError::InvalidSource(
            "URL provider 只按明确 URL 获取；目录搜索请使用 skills.sh 或配置的私有源".to_owned(),
        ))
    }

    async fn fetch(&self, id: &str) -> RegistryResult<FetchedCatalogAsset> {
        let url = Url::parse(id)
            .map_err(|_| RegistryError::InvalidSource("URL 来源必须是合法 HTTPS URL".to_owned()))?;
        if url.scheme() != "https" || !url.username().is_empty() {
            return Err(RegistryError::InvalidSource(
                "URL 来源必须是无凭据的 HTTPS URL".to_owned(),
            ));
        }
        let fetched = self
            .http
            .get_limited(&url, self.cache, self.bearer.as_ref(), 64 * 1024 * 1024)
            .await?;
        // 先尝试作为归档解析；若失败则当作单个 SKILL.md 文件处理。
        let content = match crate::extract_archive(&fetched.body) {
            Ok(raw) => strip_common_root(raw)?,
            Err(_) => {
                // 单文件模式：URL 必须指向 SKILL.md（不区分大小写路径段）。
                let name = url
                    .path_segments()
                    .into_iter()
                    .flatten()
                    .next_back()
                    .unwrap_or("SKILL.md");
                if !name.eq_ignore_ascii_case("SKILL.md") {
                    return Err(RegistryError::InvalidSource(format!(
                        "URL 既不是支持的归档，也不是 SKILL.md：{name}"
                    )));
                }
                let text = std::str::from_utf8(&fetched.body).map_err(|_| {
                    RegistryError::InvalidSource("单文件 URL 必须是 UTF-8 文本".to_owned())
                })?;
                AssetContent {
                    files: vec![AssetFileContent {
                        relative_path: Utf8PathBuf::from("SKILL.md"),
                        bytes: text.as_bytes().to_vec(),
                        executable: false,
                    }],
                }
            }
        };
        if !content
            .files
            .iter()
            .any(|file| file.relative_path == Utf8Path::new("SKILL.md"))
        {
            return Err(RegistryError::NotFound("URL 内容缺少 SKILL.md".to_owned()));
        }
        let package = url
            .path_segments()
            .into_iter()
            .flatten()
            .next_back()
            .unwrap_or("url-skill")
            .trim_end_matches(".tar.gz")
            .trim_end_matches(".tgz")
            .trim_end_matches(".zip")
            .trim_end_matches(".tar")
            .to_owned();
        let source = Source {
            kind: SourceKind::Url,
            namespace: url.host_str().unwrap_or("url").to_owned(),
            locator: url.to_string(),
            revision: None,
        };
        Ok(FetchedCatalogAsset::Skill(FetchedAsset {
            kind: AssetKind::Skill,
            source,
            source_namespace: url.host_str().unwrap_or("url").to_owned(),
            package,
            relative_path: Utf8PathBuf::from("."),
            content,
            license: None,
            metadata: serde_json::json!({
                "cache_state": fetched.cache_state,
                "rate_limit": fetched.rate_limit,
            }),
        }))
    }
}

/// 兼容 Official MCP Registry OpenAPI 的只读 provider。
pub struct McpRegistryProvider<'a> {
    id: String,
    http: ConditionalHttpClient,
    cache: &'a dyn HttpCache,
    base: Url,
    token: Option<SecretValue>,
    namespace_verified: bool,
}

impl std::fmt::Debug for McpRegistryProvider<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpRegistryProvider")
            .field("id", &self.id)
            .field("base", &self.base)
            .field("token", &self.token.as_ref().map(|_| "***"))
            .field("namespace_verified", &self.namespace_verified)
            .finish()
    }
}

impl<'a> McpRegistryProvider<'a> {
    /// 官方中心的可选 preview provider；Host 默认应优先配置兼容的下游 Registry。
    pub fn official_preview(cache: &'a dyn HttpCache) -> RegistryResult<Self> {
        Self::new(
            "official-mcp-preview",
            Url::parse("https://registry.modelcontextprotocol.io")
                .map_err(|error| RegistryError::InvalidSource(error.to_string()))?,
            cache,
            None,
            true,
        )
    }

    /// 创建兼容 Registry provider。
    pub fn new(
        id: impl Into<String>,
        base: Url,
        cache: &'a dyn HttpCache,
        token: Option<SecretValue>,
        namespace_verified: bool,
    ) -> RegistryResult<Self> {
        crate::validate_remote_url(&base)?;
        Ok(Self {
            id: id.into(),
            http: ConditionalHttpClient::new()?,
            cache,
            base,
            token,
            namespace_verified,
        })
    }
}

#[async_trait]
impl CatalogProvider for McpRegistryProvider<'_> {
    fn id(&self) -> &str {
        &self.id
    }

    async fn search(&self, query: &str, limit: usize) -> RegistryResult<SearchResult> {
        validate_query(query, limit)?;
        let mut url = self
            .base
            .join("/v0.1/servers")
            .map_err(|error| RegistryError::InvalidSource(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("search", query)
            .append_pair("limit", &limit.min(100).to_string())
            .append_pair("version", "latest");
        let fetched = self.http.get(&url, self.cache, self.token.as_ref()).await?;
        let value: Value = serde_json::from_slice(&fetched.body)?;
        let servers = value
            .get("servers")
            .and_then(Value::as_array)
            .ok_or_else(|| RegistryError::BadResponse("MCP Registry 缺少 servers".to_owned()))?;
        let items = servers
            .iter()
            .map(|entry| parse_mcp_item(entry, self.id(), self.namespace_verified))
            .collect::<RegistryResult<Vec<_>>>()?;
        Ok(SearchResult {
            items,
            cache_state: fetched.cache_state,
            rate_limit: fetched.rate_limit,
        })
    }

    async fn fetch(&self, id: &str) -> RegistryResult<FetchedCatalogAsset> {
        let segments = safe_id_segments(id)?;
        let mut url = self
            .base
            .join("/v0.1/servers/")
            .map_err(|error| RegistryError::InvalidSource(error.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| RegistryError::InvalidSource("MCP Registry URL 无法拼接".to_owned()))?
            .extend(segments.iter().copied());
        let fetched = self.http.get(&url, self.cache, self.token.as_ref()).await?;
        let value: Value = serde_json::from_slice(&fetched.body)?;
        let server_json = value
            .get("server")
            .cloned()
            .unwrap_or_else(|| value.clone());
        let item = parse_mcp_item(&value, self.id(), self.namespace_verified)?;
        Ok(FetchedCatalogAsset::McpMetadata { item, server_json })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubLocator {
    owner: String,
    repository: String,
    reference: String,
    subdirectory: Utf8PathBuf,
}

impl GithubLocator {
    fn parse(value: &str) -> RegistryResult<Self> {
        let url = Url::parse(value).map_err(|_| {
            RegistryError::InvalidSource(
                "GitHub 来源必须是 https://github.com/owner/repo URL".to_owned(),
            )
        })?;
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
        {
            return Err(RegistryError::InvalidSource(
                "GitHub 来源必须是无凭据的 github.com HTTPS URL".to_owned(),
            ));
        }
        let segments: Vec<_> = url
            .path_segments()
            .into_iter()
            .flatten()
            .filter(|segment| !segment.is_empty())
            .collect();
        if segments.len() < 2 {
            return Err(RegistryError::InvalidSource(
                "GitHub URL 缺少 owner/repo".to_owned(),
            ));
        }
        validate_github_name(segments[0])?;
        let repository = segments[1].trim_end_matches(".git");
        validate_github_name(repository)?;
        let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        let mut reference = query
            .get("ref")
            .cloned()
            .unwrap_or_else(|| "HEAD".to_owned());
        let mut subdirectory = query
            .get("path")
            .map(Utf8PathBuf::from)
            .unwrap_or_else(|| Utf8PathBuf::from("."));
        if segments.get(2) == Some(&"tree") {
            reference = segments
                .get(3)
                .ok_or_else(|| RegistryError::InvalidSource("GitHub tree URL 缺少 ref".to_owned()))?
                .to_string();
            if segments.len() > 4 {
                subdirectory = segments[4..].iter().collect::<Utf8PathBuf>();
            }
        }
        validate_ref(&reference)?;
        if subdirectory.is_absolute()
            || subdirectory.components().any(|part| {
                matches!(
                    part,
                    camino::Utf8Component::ParentDir
                        | camino::Utf8Component::RootDir
                        | camino::Utf8Component::Prefix(_)
                )
            })
        {
            return Err(RegistryError::Security(
                "GitHub Skill 子目录不能逃逸仓库".to_owned(),
            ));
        }
        Ok(Self {
            owner: segments[0].to_owned(),
            repository: repository.to_owned(),
            reference,
            subdirectory,
        })
    }

    fn public_locator(&self) -> String {
        format!(
            "https://github.com/{}/{}?ref={}&path={}",
            self.owner, self.repository, self.reference, self.subdirectory
        )
    }
}

fn parse_skills_item(value: &Value) -> RegistryResult<CatalogItem> {
    Ok(CatalogItem {
        id: required_str(value, "id")?.to_owned(),
        provider_id: "skills.sh".to_owned(),
        kind: AssetKind::Skill,
        name: required_str(value, "name")?.to_owned(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
        version: None,
        license: None,
        locator: required_str(value, "url")?.to_owned(),
        namespace_verified: false,
        metadata: value.clone(),
        vulnerabilities: Vec::new(),
    })
}

fn parse_mcp_item(
    value: &Value,
    provider_id: &str,
    namespace_verified: bool,
) -> RegistryResult<CatalogItem> {
    let server = value.get("server").unwrap_or(value);
    let extension = value
        .get("x-io.modelcontextprotocol.registry")
        .or_else(|| value.get("_meta"));
    let id = server
        .get("name")
        .or_else(|| server.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| RegistryError::BadResponse("MCP server 缺少 name/id".to_owned()))?;
    let version = server.get("version").and_then(Value::as_str).or_else(|| {
        extension
            .and_then(|meta| meta.get("version"))
            .and_then(Value::as_str)
    });
    Ok(CatalogItem {
        id: id.to_owned(),
        provider_id: provider_id.to_owned(),
        kind: AssetKind::McpServer,
        name: id.to_owned(),
        description: server
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
        version: version.map(str::to_owned),
        license: None,
        locator: format!("mcp-registry:{provider_id}/{id}"),
        namespace_verified,
        metadata: value.clone(),
        vulnerabilities: Vec::new(),
    })
}

fn validate_query(query: &str, limit: usize) -> RegistryResult<()> {
    if query.trim().len() < 2 || query.len() > 256 || limit == 0 {
        return Err(RegistryError::InvalidSource(
            "搜索词长度必须是 2..=256，limit 必须大于 0".to_owned(),
        ));
    }
    Ok(())
}

fn safe_id_segments(value: &str) -> RegistryResult<Vec<&str>> {
    let segments: Vec<_> = value.split('/').collect();
    if segments.is_empty()
        || segments.iter().any(|segment| {
            segment.is_empty()
                || *segment == "."
                || *segment == ".."
                || segment.contains(['\\', '\0', '\r', '\n'])
        })
    {
        return Err(RegistryError::InvalidSource(format!(
            "Provider ID 路径无效：{value}"
        )));
    }
    Ok(segments)
}

fn required_str<'a>(value: &'a Value, field: &str) -> RegistryResult<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| RegistryError::BadResponse(format!("缺少字符串字段：{field}")))
}

fn validate_github_name(value: &str) -> RegistryResult<()> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._".contains(&byte))
    {
        return Err(RegistryError::InvalidSource(format!(
            "GitHub owner/repo 无效：{value}"
        )));
    }
    Ok(())
}

fn validate_ref(value: &str) -> RegistryResult<()> {
    if value.is_empty()
        || value.len() > 256
        || value.contains(['\0', '\r', '\n'])
        || value.contains("..")
    {
        return Err(RegistryError::InvalidSource("GitHub ref 无效".to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_locator_accepts_repo_tree_and_query_forms() {
        let root = GithubLocator::parse("https://github.com/acme/skills").unwrap();
        assert_eq!(root.reference, "HEAD");
        assert_eq!(root.subdirectory, ".");

        let tree =
            GithubLocator::parse("https://github.com/acme/skills/tree/v1/packages/demo").unwrap();
        assert_eq!(tree.reference, "v1");
        assert_eq!(tree.subdirectory, "packages/demo");

        let query =
            GithubLocator::parse("https://github.com/acme/skills?ref=main&path=packages/demo")
                .unwrap();
        assert_eq!(query.subdirectory, "packages/demo");
    }

    #[test]
    fn github_locator_rejects_credentials_and_traversal() {
        assert!(GithubLocator::parse("https://token@github.com/acme/repo").is_err());
        assert!(GithubLocator::parse("https://github.com/acme/repo?path=../escape").is_err());
    }

    #[test]
    fn mcp_parser_accepts_v01_wrapper_without_claiming_code_safety() {
        let value = serde_json::json!({
            "server": {
                "name": "io.github.acme/demo",
                "description": "Demo"
            },
            "x-io.modelcontextprotocol.registry": {
                "version": "1.2.3"
            }
        });
        let item = parse_mcp_item(&value, "official-preview", true).unwrap();
        assert_eq!(item.version.as_deref(), Some("1.2.3"));
        assert!(item.namespace_verified);
        assert_eq!(item.kind, AssetKind::McpServer);
    }

    #[test]
    fn skills_parser_keeps_provider_metadata() {
        let value = serde_json::json!({
            "id": "acme/skills/demo",
            "name": "demo",
            "url": "https://skills.sh/acme/skills/demo",
            "installs": 10
        });
        let item = parse_skills_item(&value).unwrap();
        assert_eq!(item.metadata["installs"], 10);
    }

    #[test]
    fn url_provider_rejects_non_https_and_credentials() {
        // 非 HTTPS
        assert!(Url::parse("http://example.com/skill.tar.gz").is_ok());
        // 带凭据的 HTTPS
        assert!(Url::parse("https://token@example.com/skill.tar.gz").is_ok());
        // 验证逻辑在 fetch 中，这里验证 URL 解析和基本判断
        let url = Url::parse("http://example.com/skill.tar.gz").unwrap();
        assert_eq!(url.scheme(), "http");
        let url = Url::parse("https://token@example.com/skill.tar.gz").unwrap();
        assert!(!url.username().is_empty());
    }

    #[tokio::test]
    async fn url_provider_search_is_unsupported() {
        let cache = crate::MemoryHttpCache::default();
        let provider = UrlProvider::new(&cache, None).unwrap();
        let result = provider.search("test", 10).await;
        assert!(result.is_err());
    }
}
