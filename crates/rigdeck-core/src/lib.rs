//! RigDeck 共享核心。
//!
//! 该 crate 只描述跨 Agent 通用的领域语义：资产、修订、分配、投影、计划、
//! 冲突与事务。这里故意不出现 Claude/Codex 等 Agent 名称分支；具体差异由
//! `rigdeck-adapter-sdk` 的运行时适配器表达。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod hash;
mod merge;
mod model;
mod plan;
mod refresh;
mod transaction;

pub use error::{CoreError, CoreResult};
pub use hash::{normalized_hash, verify_hashes, ContentHash};
pub use merge::{merge_json, merge_text, MergeResult};
pub use model::*;
pub use plan::*;
pub use refresh::*;
pub use transaction::*;

/// Core 公共协议版本。
///
/// 持久化层和 CLI JSON 会把该值写入 envelope。破坏兼容性的字段变化必须提升
/// 主版本，而不是依赖调用方猜测结构。
pub const CORE_PROTOCOL_VERSION: u32 = 1;
