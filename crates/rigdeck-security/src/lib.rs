//! RigDeck Security — Audit, sandbox, secret management, archive safety.
//!
//! Responsibilities:
//! - Static audit of Skill content (credential harvesting, hidden payloads,
//!   unsafe install instructions, prompt-injection patterns).
//! - Archive bomb limits, maximum file counts/sizes, path normalization, symlink policy.
//! - Secret redaction from logs, diagnostics, crash output, plans, and UI copy.
//! - Sandbox/constraint for third-party adapter helper processes.
//! - SBOM generation for desktop and CLI releases.

#![warn(missing_docs)]
