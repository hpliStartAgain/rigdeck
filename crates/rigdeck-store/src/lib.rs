//! RigDeck 本地持久化。
//!
//! SQLite 保存可重建的 metadata 与审计状态；内容寻址对象库保存不可变修订和
//! 文件备份。该 crate 不接触系统钥匙串明文，只持久化 Core 中的 `SecretRef`。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod database;
mod error;
mod object_store;

pub use database::*;
pub use error::*;
pub use object_store::*;

mod embedded {
    use refinery::embed_migrations;

    embed_migrations!("migrations");
}

/// SQLite schema 版本。
pub const STORE_SCHEMA_VERSION: u32 = 2;
