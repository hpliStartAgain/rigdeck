# ADR-0005: Storage Architecture

- **Status**: Accepted
- **Date**: 2026-07-11

## Context

RigDeck needs local persistence for metadata, inventory, assignments, baselines, conflict records, audit events, and immutable asset revisions. Secrets must never be stored in plaintext.

## Decision

### SQLite (Metadata)

- **Tool**: rusqlite (synchronous, bundled) + refinery (migrations).
- **Contents**: inventory, assignments, baselines, conflict records, migrations, audit events.
- **Secrets**: Only `SecretRef` identifiers are stored. Plaintext lives in OS keychains.

### Content-Addressed Object Store (Immutable Revisions)

- **Hash**: Blake3.
- **Contents**: Immutable asset revisions and operation backups.
- **Layout**: Sharded by first 2 hex chars of hash (e.g., `objects/ab/abcdef...`).
- **Deduplication**: Identical content is stored once.

### OS Keychain (Secrets)

- **Windows**: Windows Credential Manager.
- **macOS**: Keychain.
- **Access**: Plaintext materialized only after high-risk confirmation when an Agent cannot reference a secret indirectly.

### Recovery

- Rebuild inventory from Agent-native state if cache/index is lost.
- Database migration failure leaves previous database usable.
- Backup and restore procedures for both SQLite and object store.

## Consequences

- Test secrets must not appear in SQLite, logs, backups, crash reports, or JSON CLI output.
- Object store deduplication saves disk space for shared skills across agents.
- Keychain dependency requires platform-specific implementations.
