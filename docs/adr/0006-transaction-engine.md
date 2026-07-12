# ADR-0006: Transaction Engine

- **Status**: Accepted
- **Date**: 2026-07-11

## Context

Every mutation in RigDeck must be planned, previewed, backed up, applied, verified, audited, and recoverable. Partial failures must roll back to the exact pre-apply state.

## Decision

### DeploymentPlan

Generated before every mutation. Contains:
- Operation preconditions
- Input hashes (source + target)
- Target paths
- Rendered diff
- Compatibility losses
- Risk level
- Rollback location (backup path)

### Plan Invalidation

A plan is invalidated if any source or target hash changes before apply.

### Atomic Apply

- Apply changes atomically wherever the OS/filesystem permits.
- Back up every overwritten or removed target before mutation.
- Roll back both filesystem and database state after injected partial failures.

### Idempotency

Repeated plan/apply operations are idempotent. Reapplying a successful assignment produces an empty plan.

### Format Preservation

Preserve unknown configuration keys, comments, line endings, encoding, and user formatting where the target format permits.

## Consequences

- Failure injected at every write step must restore exact pre-apply state.
- GUI and CLI generate byte-equivalent plans for identical inputs.
- Backup storage is required for every mutation (content-addressed store).
