//! 可选 helper 的 JSON-RPC 2.0 消息结构。

use serde::{Deserialize, Serialize};

/// JSON-RPC 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// 固定为 `2.0`。
    pub jsonrpc: String,
    /// 调用 ID。
    pub id: serde_json::Value,
    /// 方法名。
    pub method: String,
    /// 不含 secret 的参数。
    pub params: serde_json::Value,
}

/// JSON-RPC 成功响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcSuccess {
    /// 固定为 `2.0`。
    pub jsonrpc: String,
    /// 调用 ID。
    pub id: serde_json::Value,
    /// 结果。
    pub result: serde_json::Value,
}

/// JSON-RPC 错误对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// 标准或 Adapter 扩展错误码。
    pub code: i64,
    /// 错误说明。
    pub message: String,
    /// 可选结构化详情。
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 错误响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcFailure {
    /// 固定为 `2.0`。
    pub jsonrpc: String,
    /// 调用 ID。
    pub id: serde_json::Value,
    /// 错误。
    pub error: JsonRpcError,
}
