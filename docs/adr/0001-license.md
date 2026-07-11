# ADR-0001: License Selection

- **Status**: Accepted
- **Date**: 2026-07-11

## Context

RigDeck is a local-first desktop application for managing Agent Skills, prompts, and MCP servers. It needs an open source license that encourages adoption while keeping requirements simple.

## Decision

Use the **MIT License**.

## Rationale

- Most permissive common license — allows commercial use, modification, distribution, and private use.
- No copyleft requirements — downstream users can integrate without license concerns.
- Simple and well-understood — minimal legal complexity.
- Compatible with all planned dependencies (Tauri = Apache-2.0/MIT, React = MIT, Rust crates mostly MIT/Apache).

## Alternatives Considered

- **Apache-2.0**: Includes patent grant clause, slightly more complex. Considered but MIT is simpler for a local tool.
- **GPL-3.0**: Strong copyleft would limit adoption. Not suitable for a tool ecosystem.

## Consequences

- All contributors retain copyright, grant MIT permissions.
- No patent grant (unlike Apache-2.0) — acceptable for a local tool with no patent-sensitive components.
