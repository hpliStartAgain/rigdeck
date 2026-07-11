//! RigDeck Adapter SDK — Versioned adapter contract and test kit.
//!
//! Defines:
//! - `adapter.json` JSON Schema for declarative adapter metadata.
//! - Methods: `describe`, `detect`, `scan`, `validate_asset`, `render`,
//!   `plan_install`, `plan_update`, `plan_remove`, `verify`, `health`.
//! - Optional JSON-RPC 2.0 over stdio for behavior that cannot be expressed declaratively.
//!
//! Adapters never write files directly. They return projections and operations
//! to the core planner.

#![warn(missing_docs)]
