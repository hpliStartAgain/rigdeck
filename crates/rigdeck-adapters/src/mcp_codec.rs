//! 统一 MCP 领域模型到各原生条目格式的纯编码器。
//!
//! 编码器永远不会读取钥匙串。`SecretRef` 会被写成不可执行的占位符，并产生阻断型
//! 兼容损失；只有后续高风险物化步骤才能把它转换成目标支持的环境变量引用或短暂
//! 明文。这一分层保证预览、日志和对象库里不会意外出现凭据。

use std::collections::BTreeMap;

use rigdeck_adapter_sdk::{AdapterError, AdapterErrorCode, AdapterResult};
use rigdeck_core::{BindingValue, CompatibilityLoss, McpServerSpec, McpTransport, OAuthMetadata};
use serde_json::{json, Map, Value};

/// 编码后的单个原生 MCP 条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedMcpEntry {
    /// 以 JSON 表示的原生条目；结构化补丁器会转换到 JSONC/TOML/YAML。
    pub bytes: Vec<u8>,
    /// 目标无法无损表达或需要后续高风险处理的语义。
    pub compatibility_losses: Vec<CompatibilityLoss>,
}

/// 按 manifest codec ID 编码 MCP 条目。
pub fn encode_mcp_entry(codec_id: &str, server: &McpServerSpec) -> AdapterResult<EncodedMcpEntry> {
    validate_server_name(&server.server_name)?;
    let mut losses = Vec::new();
    let value = match codec_id {
        "claude-mcp-v1" | "antigravity-mcp-v1" => encode_claude_family(server, &mut losses),
        "codex-mcp-toml-v1" => encode_codex(server, &mut losses),
        "opencode-jsonc-v1" => encode_opencode(server, &mut losses),
        "hermes-yaml-v1" => encode_hermes(server, &mut losses),
        "pi-mcp-extension-v1" | "devin-manual-export-v1" => {
            return Err(AdapterError::new(
                AdapterErrorCode::ManualRequired,
                "该 MCP codec 只能生成经人工确认的交接产物",
            ));
        }
        _ => {
            return Err(AdapterError::new(
                AdapterErrorCode::UnsupportedCapability,
                format!("未知 MCP codec：{codec_id}"),
            ));
        }
    };
    let bytes = serde_json::to_vec_pretty(&value).map_err(|error| {
        AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!("原生 MCP 条目无法序列化：{error}"),
        )
    })?;
    Ok(EncodedMcpEntry {
        bytes,
        compatibility_losses: losses,
    })
}

fn encode_claude_family(server: &McpServerSpec, losses: &mut Vec<CompatibilityLoss>) -> Value {
    let mut object = Map::new();
    match &server.transport {
        McpTransport::Stdio { command, args, env } => {
            object.insert("type".to_owned(), json!("stdio"));
            object.insert("command".to_owned(), json!(command));
            object.insert("args".to_owned(), json!(args));
            object.insert("env".to_owned(), encode_bindings(env, losses));
        }
        McpTransport::StreamableHttp { url, headers } => {
            object.insert("type".to_owned(), json!("http"));
            object.insert("url".to_owned(), json!(url));
            object.insert("headers".to_owned(), encode_bindings(headers, losses));
        }
    }
    object.insert("enabled".to_owned(), json!(server.enabled));
    insert_timeout(&mut object, "timeout", server.timeout_ms);
    insert_oauth(&mut object, server.oauth.as_ref());
    insert_tool_filters(&mut object, server);
    Value::Object(object)
}

fn encode_codex(server: &McpServerSpec, losses: &mut Vec<CompatibilityLoss>) -> Value {
    let mut object = Map::new();
    match &server.transport {
        McpTransport::Stdio { command, args, env } => {
            object.insert("command".to_owned(), json!(command));
            object.insert("args".to_owned(), json!(args));
            object.insert("env".to_owned(), encode_bindings(env, losses));
        }
        McpTransport::StreamableHttp { url, headers } => {
            object.insert("url".to_owned(), json!(url));
            object.insert("http_headers".to_owned(), encode_bindings(headers, losses));
        }
    }
    object.insert("enabled".to_owned(), json!(server.enabled));
    if let Some(timeout_ms) = server.timeout_ms {
        object.insert(
            "tool_timeout_sec".to_owned(),
            json!((timeout_ms as f64 / 1000.0).max(0.001)),
        );
    }
    if !server.allowed_tools.is_empty() {
        object.insert("enabled_tools".to_owned(), json!(server.allowed_tools));
    }
    if !server.denied_tools.is_empty() {
        object.insert("disabled_tools".to_owned(), json!(server.denied_tools));
    }
    if let Some(oauth) = &server.oauth {
        object.insert("oauth_resource".to_owned(), json!(oauth.issuer));
        if !oauth.client_id.is_empty() || !oauth.scopes.is_empty() {
            losses.push(CompatibilityLoss {
                code: "codex_oauth_metadata_partial".to_owned(),
                message: "Codex 原生配置只直接表达 OAuth resource；client_id/scopes 由登录流程管理"
                    .to_owned(),
                blocking: false,
            });
        }
    }
    Value::Object(object)
}

fn encode_opencode(server: &McpServerSpec, losses: &mut Vec<CompatibilityLoss>) -> Value {
    let mut object = Map::new();
    match &server.transport {
        McpTransport::Stdio { command, args, env } => {
            let mut command_line = Vec::with_capacity(args.len() + 1);
            command_line.push(command.clone());
            command_line.extend(args.iter().cloned());
            object.insert("type".to_owned(), json!("local"));
            object.insert("command".to_owned(), json!(command_line));
            object.insert("environment".to_owned(), encode_bindings(env, losses));
        }
        McpTransport::StreamableHttp { url, headers } => {
            object.insert("type".to_owned(), json!("remote"));
            object.insert("url".to_owned(), json!(url));
            object.insert("headers".to_owned(), encode_bindings(headers, losses));
        }
    }
    object.insert("enabled".to_owned(), json!(server.enabled));
    insert_timeout(&mut object, "timeout", server.timeout_ms);
    if let Some(oauth) = &server.oauth {
        object.insert(
            "oauth".to_owned(),
            json!({
                "issuer": oauth.issuer,
                "clientId": oauth.client_id,
                "scopes": oauth.scopes,
            }),
        );
    }
    if !server.allowed_tools.is_empty() || !server.denied_tools.is_empty() {
        losses.push(CompatibilityLoss {
            code: "opencode_tool_filters_require_permissions".to_owned(),
            message: "OpenCode 的工具过滤由 permission 表面表达，不能塞进单个 MCP server 条目"
                .to_owned(),
            blocking: false,
        });
    }
    Value::Object(object)
}

fn encode_hermes(server: &McpServerSpec, losses: &mut Vec<CompatibilityLoss>) -> Value {
    let mut object = Map::new();
    match &server.transport {
        McpTransport::Stdio { command, args, env } => {
            object.insert("transport".to_owned(), json!("stdio"));
            object.insert("command".to_owned(), json!(command));
            object.insert("args".to_owned(), json!(args));
            object.insert("env".to_owned(), encode_bindings(env, losses));
        }
        McpTransport::StreamableHttp { url, headers } => {
            object.insert("transport".to_owned(), json!("http"));
            object.insert("url".to_owned(), json!(url));
            object.insert("headers".to_owned(), encode_bindings(headers, losses));
        }
    }
    object.insert("enabled".to_owned(), json!(server.enabled));
    insert_timeout(&mut object, "timeout_ms", server.timeout_ms);
    if server.oauth.is_some() || !server.allowed_tools.is_empty() || !server.denied_tools.is_empty()
    {
        losses.push(CompatibilityLoss {
            code: "hermes_mcp_metadata_partial".to_owned(),
            message: "Hermes 原生 mcp_servers 不完整表达统一模型中的 OAuth 或工具过滤字段"
                .to_owned(),
            blocking: false,
        });
    }
    Value::Object(object)
}

fn encode_bindings(
    bindings: &BTreeMap<String, BindingValue>,
    losses: &mut Vec<CompatibilityLoss>,
) -> Value {
    let mut object = Map::new();
    let mut contains_secret = false;
    for (key, value) in bindings {
        let value = match value {
            BindingValue::Literal(value) => Value::String(value.clone()),
            BindingValue::Secret(reference) => {
                contains_secret = true;
                Value::String(format!("rigdeck-secret-ref://{}", reference.as_str()))
            }
        };
        object.insert(key.clone(), value);
    }
    if contains_secret
        && !losses
            .iter()
            .any(|loss| loss.code == "secret_materialization_required")
    {
        losses.push(CompatibilityLoss {
            code: "secret_materialization_required".to_owned(),
            message: "目标配置需要 SecretRef；应用前必须选择环境变量间接引用或完成高风险临时物化"
                .to_owned(),
            blocking: true,
        });
    }
    Value::Object(object)
}

fn insert_timeout(object: &mut Map<String, Value>, key: &str, timeout_ms: Option<u64>) {
    if let Some(timeout_ms) = timeout_ms {
        object.insert(key.to_owned(), json!(timeout_ms));
    }
}

fn insert_oauth(object: &mut Map<String, Value>, oauth: Option<&OAuthMetadata>) {
    if let Some(oauth) = oauth {
        object.insert(
            "oauth".to_owned(),
            json!({
                "issuer": oauth.issuer,
                "clientId": oauth.client_id,
                "scopes": oauth.scopes,
            }),
        );
    }
}

fn insert_tool_filters(object: &mut Map<String, Value>, server: &McpServerSpec) {
    if !server.allowed_tools.is_empty() {
        object.insert("allowedTools".to_owned(), json!(server.allowed_tools));
    }
    if !server.denied_tools.is_empty() {
        object.insert("deniedTools".to_owned(), json!(server.denied_tools));
    }
}

fn validate_server_name(value: &str) -> AdapterResult<()> {
    let valid = !value.trim().is_empty()
        && value.len() <= 128
        && !value.contains(['\0', '\r', '\n', '/', '\\']);
    if !valid {
        return Err(AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!("MCP server name 无效：{value}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rigdeck_core::{McpTransport, SecretRef};

    use super::*;

    fn stdio() -> McpServerSpec {
        McpServerSpec {
            server_name: "demo".to_owned(),
            transport: McpTransport::Stdio {
                command: "demo-server".to_owned(),
                args: vec!["--safe".to_owned()],
                env: BTreeMap::from([
                    (
                        "MODE".to_owned(),
                        BindingValue::Literal("read-only".to_owned()),
                    ),
                    (
                        "TOKEN".to_owned(),
                        BindingValue::Secret(SecretRef::new("vault:demo-token").unwrap()),
                    ),
                ]),
            },
            enabled: true,
            timeout_ms: Some(5_000),
            oauth: None,
            allowed_tools: vec!["read".to_owned()],
            denied_tools: vec!["delete".to_owned()],
        }
    }

    #[test]
    fn opencode_uses_command_array_and_never_embeds_secret() {
        let encoded = encode_mcp_entry("opencode-jsonc-v1", &stdio()).unwrap();
        let text = String::from_utf8(encoded.bytes).unwrap();
        assert!(text.contains("\"command\": ["));
        assert!(text.contains("rigdeck-secret-ref://vault:demo-token"));
        assert!(!text.contains("actual-secret"));
        assert!(encoded
            .compatibility_losses
            .iter()
            .any(|loss| loss.code == "secret_materialization_required" && loss.blocking));
    }

    #[test]
    fn codex_maps_timeout_and_tool_filters_to_documented_fields() {
        let encoded = encode_mcp_entry("codex-mcp-toml-v1", &stdio()).unwrap();
        let value: Value = serde_json::from_slice(&encoded.bytes).unwrap();
        assert_eq!(value["tool_timeout_sec"], json!(5.0));
        assert_eq!(value["enabled_tools"], json!(["read"]));
        assert_eq!(value["disabled_tools"], json!(["delete"]));
    }

    #[test]
    fn unknown_codec_fails_closed() {
        let error = encode_mcp_entry("unknown", &stdio()).unwrap_err();
        assert_eq!(error.code, AdapterErrorCode::UnsupportedCapability);
    }
}
