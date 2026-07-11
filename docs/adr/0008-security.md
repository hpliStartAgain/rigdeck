# ADR-0008: Security Model

- **Status**: Proposed
- **Date**: 2026-07-11

## Context

RigDeck handles untrusted content (remote skills, archives, MCP configurations) and must protect against path traversal, symlink escape, archive bombs, prompt injection, and credential leakage.

## Decision

### Threat Model (STRIDE)

- **Sources**: untrusted remote content → validate before storage.
- **Archives**: bomb limits, max file count/size, path normalization, symlink policy.
- **Adapters**: sandboxed helper processes, explicit trust decision required.
- **MCP credentials**: OS keychain only, SecretRef in SQLite, redaction in all outputs.
- **Updaters**: signed manifests, reject invalid/downgraded updates.
- **Filesystem writes**: only paths in approved plans, backup before mutation.

### Static Audit

Detect suspicious Skill content:
- Credential harvesting patterns
- Hidden executable payloads
- Unsafe install instructions
- Prompt-injection patterns

Disclose findings without claiming perfect safety.

### Secret Redaction

Redact secrets from: logs, diagnostics, crash output, plans, UI copy operations, JSON CLI output, exported bundles.

### SBOM

Generate Software Bill of Materials for desktop and CLI releases.

## Consequences

- No unresolved critical/high dependency vulnerability without approved written exception.
- Security fixtures block traversal, symlink escape, malformed archive, and untrusted helper execution.
- Chaos tests prove rollback or produce deterministic recovery instruction.
