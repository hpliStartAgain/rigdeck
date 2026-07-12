//! RigDeck Adapter SDK。
//!
//! Adapter 负责描述 Agent 的原生表面、检测实例、扫描与渲染，但永远不直接写文件。
//! 对外部 Agent 的差异全部通过该版本化契约表达，Core 无需认识任何 Agent 名称。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod contract;
mod error;
mod manifest;
mod package;
mod rpc;
mod toolkit;

pub use contract::*;
pub use error::*;
pub use manifest::*;
pub use package::*;
pub use rpc::*;
pub use toolkit::*;

/// 当前 Adapter 协议版本。
pub const ADAPTER_PROTOCOL_VERSION: u32 = 1;

/// 随 crate 发布的 `adapter.json` JSON Schema。
pub const ADAPTER_JSON_SCHEMA: &str = include_str!("../schema/adapter.schema.json");
