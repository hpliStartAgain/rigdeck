//! RigDeck Store — SQLite metadata + content-addressed object store.
//!
//! - SQLite: inventory, assignments, baselines, conflict records, migrations, audit events.
//! - Content-addressed store: immutable asset revisions and operation backups (Blake3).
//! - Secrets: stored in OS keychains (Windows Credential Manager / macOS Keychain).
//!   Only `SecretRef` identifiers are persisted in SQLite.

#![warn(missing_docs)]
