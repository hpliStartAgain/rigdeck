//! RigDeck 内置 Agent Adapters。
//!
//! 七个首发 Agent 的差异被编译为 manifest 数据，执行逻辑由同一个声明式 Adapter
//! 解释器完成。这样既避免复制危险的文件操作代码，也能用同一套契约测试验证所有
//! 路径；真正的写入仍只由 Core Planner 执行。
//!
//! Each adapter implements the contract defined in `rigdeck-adapter-sdk`.
//! New agent types can be added as adapter packages without recompiling core.

#![warn(missing_docs)]

mod bounded;
mod builtin;
mod materialize;
mod mcp_codec;
mod refresh;
mod structured;
mod watch;

pub use bounded::*;
pub use builtin::*;
pub use materialize::*;
pub use mcp_codec::*;
pub use refresh::*;
pub use structured::*;
pub use watch::*;
