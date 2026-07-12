//! RigDeck 资产来源与安全导入。
//!
//! 远端来源只是可替换的 metadata provider，本地库存、卸载、恢复和冲突处理不依赖
//! 网络。所有归档与目录都先经过路径、symlink、大小和文件数检查，且检查 Skill 时
//! 永不执行其中脚本。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod archive;
mod error;
mod http;
mod importer;
mod model;
mod providers;

pub use archive::*;
pub use error::*;
pub use http::*;
pub use importer::*;
pub use model::*;
pub use providers::*;
