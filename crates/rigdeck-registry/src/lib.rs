//! RigDeck Registry — Source providers for asset discovery.
//!
//! Providers:
//! - skills.sh
//! - GitHub repositories/subdirectories
//! - Local directories
//! - URLs
//! - Archives
//! - Configurable private sources
//! - Official MCP Registry (preview metadata)
//!
//! Remote metadata is cached with ETag/conditional requests.
//! Rate limits are handled explicitly and never corrupt cached inventory.

#![warn(missing_docs)]
