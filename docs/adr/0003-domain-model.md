# ADR-0003: Domain Model Design

- **Status**: Accepted
- **Date**: 2026-07-11

## Context

RigDeck must manage three kinds of assets (Skills, prompts, MCP servers) through one unified domain model, with immutable revisions, provenance tracking, and three-way conflict detection.

## Decision

### Core Types

- **Asset**: kind (`skill` | `prompt` | `mcp_server`), identity (namespace + repo + path + declared name).
- **AssetRevision**: immutable, normalized hash (Blake3), raw hash, source provenance, license, audit result.
- **Source**: GitHub, skills.sh, MCP registries, local folders, archives, private sources.
- **AgentAdapter**: runtime adapter implementing the adapter contract.
- **AgentInstance**: detected agent installation with version, paths, profiles.
- **Assignment**: asset → agent instance + scope binding.
- **Projection**: rendered representation of an asset for a specific agent's native format.
- **DeploymentPlan**: file-level operations with preconditions, hashes, diffs, risk, rollback.
- **DeploymentSnapshot**: baseline state after a successful apply.
- **Conflict**: typed conflict with cause, affected projections, risk, resolution options.
- **SecretRef**: identifier pointing to OS keychain entry.
- **AuditEvent**: immutable operation log entry.

### Identity Rule

A Skill is identified by `source namespace + repository/package + relative path + declared name`, not directory name alone. This prevents name collisions and provenance confusion.

### Versioning

Every persistent schema and public protocol is versioned from the first release. Schema migrations via refinery.

## Consequences

- Core contains no Agent-name conditional branches.
- Three-way conflict detection requires base (last deployment snapshot), ours (RigDeck plan), theirs (agent-side current state).
- Immutable revisions enable content-addressed storage and deduplication.
