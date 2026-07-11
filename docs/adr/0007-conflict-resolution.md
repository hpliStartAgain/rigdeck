# ADR-0007: Conflict Resolution

- **Status**: Accepted
- **Date**: 2026-07-11

## Context

RigDeck must detect and resolve conflicts between RigDeck-managed state, agent-side changes, and source updates. The conflict taxonomy includes 10+ conflict types.

## Decision

### Algorithm

Use **diff3 three-way merge** as the standard algorithm:
- **Base**: last deployment snapshot.
- **Ours**: RigDeck's planned revision.
- **Theirs**: agent-side current state.

### Automatic Resolution

Automatically resolve:
- Identical content (duplicates)
- One-sided changes
- Safe path normalization
- Non-overlapping configuration keys

### Manual Resolution

Require user review for:
- Overlapping content changes
- Semantic differences

### Resolution Options

1. Keep RigDeck revision
2. Import Agent revision
3. Keep per-Agent fork
4. Rename and coexist
5. Three-way merge (diff3)
6. Per-file selection
7. Abandon plan
8. Restore backup

### Hard Rule

**Never** label force-overwrite as conflict resolution.

### Persistence

After resolution: persist decision, new baseline, and audit event.

## Consequences

- Every conflict contains cause, affected projections, risk, and at least one valid next action.
- Resolving a conflict then refreshing produces a stable clean or intentionally divergent state.
- diff3 implementation needs to handle text, JSON, YAML, TOML, and frontmatter formats.
