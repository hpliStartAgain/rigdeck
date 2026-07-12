//! RigDeck 安全边界。
//!
//! 当前模块提供系统钥匙串抽象与结构化脱敏；归档/路径/Skill 静态审计在同一 crate
//! 中逐步扩展。这里的类型刻意不为 secret 实现序列化，避免误写 SQLite/JSON。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod redact;
mod secret;

pub use redact::*;
pub use secret::*;
