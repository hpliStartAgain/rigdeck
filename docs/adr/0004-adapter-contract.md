# ADR-0004: Adapter Contract

- **Status**: Proposed
- **Date**: 2026-07-11

## Context

RigDeck must support multiple coding agents (Claude Code, Codex, OpenCode, Hermes, Devin, Antigravity, Pi) without hardcoding agent-specific logic in core. New agents must be addable as adapter packages.

## Decision

### Declarative Metadata

Each adapter ships an `adapter.json` JSON Schema containing:
- Adapter ID, version, platforms
- Detection rules (path patterns, file markers, version extraction)
- Asset capabilities (which kinds/scope/operations are supported)
- Native formats and codecs
- Limitations and official documentation links

### Methods

Adapters implement: `describe`, `detect`, `scan`, `validate_asset`, `render`, `plan_install`, `plan_update`, `plan_remove`, `verify`, `health`.

### Optional RPC

For behavior that cannot be expressed declaratively, adapters may provide JSON-RPC 2.0 over stdio helpers. Running a third-party helper requires an explicit trust decision.

### Hard Rules

- Adapters **never** write files directly. They return projections and operations to the core planner.
- Core contains **no** agent-name conditional branches.
- Adapter compatibility, error codes, and protocol negotiation are versioned.

### Developer Commands

`rigdeck adapter scaffold`, `validate`, `test`, `pack`.

## Consequences

- A mock Agent can be detected by installing an adapter package without recompiling core.
- Adapter Contract Test Kit ensures compatibility.
- Path traversal, symlink escape, malformed manifests, and incompatible protocol versions fail safely.
