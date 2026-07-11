---
status: active
branch: main
owner: claude-code
updated: 2026-07-11
tier: COMPLEX
---

# RigDeck — TODO

## Phase -1: Resolved Human-Decision Blockers

All items below were blocking items that required human intervention. They have been resolved as of 2026-07-11.

| # | Decision | Resolution | Date |
|---|----------|------------|------|
| 1 | Open source license | **MIT** | 2026-07-11 |
| 2 | MVP scope | **Direct to GA** — all 9 phases, 7 adapters, no intermediate slice | 2026-07-11 |
| 3 | Code signing | **Not signed for now** — no certificate purchased; users manually trust; signing before GA | 2026-07-11 |
| 4 | Reference hardware for performance budgets | **Mid-range Windows**: Intel i5-12400 / 16GB RAM / NVMe SSD; macOS comparison on M1 | 2026-07-11 |
| 5 | Name conflict with rigdeck.ca | **Continue using RigDeck** — different industry (developer tools vs diesel repair SaaS); trademark search before public launch | 2026-07-11 |
| 6 | Repository location | `C:\Users\19682\Documents\rigdeck` | 2026-07-11 |
| 7 | Brand primary colors | Deep blue `#1B4D7E` + teal accent `#2DD4BF` | 2026-07-11 |
| 8 | Logo style | Geometric line style — rounded D outline with three connected modules (Skill/Prompt/MCP) | 2026-07-11 |

### Resolved Technical Decisions

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| T1 | UI component library | shadcn/ui + Tailwind CSS | Design-token driven, multi-theme support, high customizability |
| T2 | State management | Zustand | Lightweight, TypeScript-friendly, low learning curve |
| T3 | SQLite access + migration | rusqlite + refinery | Synchronous, no async overhead, suitable for local tools |
| T4 | Content-addressed hash | Blake3 | 3-5x faster than SHA-256, modern design |
| T5 | CLI framework | clap v4 | Mature, derive macros, subcommand/help/completion support |
| T6 | File watcher | notify | Cross-platform, mature Rust ecosystem |
| T7 | Three-way merge | diff3 | Standard algorithm, same as Git, proven reliability |
| T8 | Frontend build tool | Vite | Mature ecosystem, fast HMR, Tauri recommended |
| T9 | Desktop shell | Tauri | Small binary, low memory, system WebView |
| T10 | Frontend language | React + TypeScript | Type safety, large ecosystem |

### Remaining Pre-Launch Actions (Non-Blocking for Development)

- [ ] Formal trademark search before public announcement
- [ ] Purchase Windows code-signing certificate before GA
- [ ] Enroll in Apple Developer Program for Developer ID + notarization before GA
- [ ] Finalize logo SVG + icon set (geometric line style, rounded D + 3 modules)
- [ ] Reserve domain name (rigdeck.app or similar — rigdeck.ca is taken)

---

## Current Task: RigDeck — Independent Multi-Agent Capability Manager

Status: planned
Tier: COMPLEX
Updated: 2026-07-11
Target repository: new standalone repository; this document is only a planning record

### Product Definition

- Project name: **RigDeck**
- English slogan: **One deck. Every agent, perfectly equipped.**
- Chinese slogan: **一处装配，让每个 Agent 各就其位。**
- Product form: local-first Windows/macOS desktop application built with Tauri, plus a standalone CLI and a meta-management Skill.
- The existing `agent-skill-registry` is an anti-pattern/reference and optional migration fixture only. Do not use it as the new application's backend, canonical store, package layout, or runtime dependency.
- "Greenfield/no existing state" means no old application state must be retained. RigDeck itself must persist the minimum local metadata required for inventory, three-way conflict detection, reliable uninstall, audit, backup, and rollback.

### Non-Negotiable Outcomes

- [ ] Manage Agent Skills, global prompts/rules/instruction files, and MCP servers through one domain model.
- [ ] Cover discover, inspect, audit, select, assign, plan, install, enable/disable, refresh, update, resolve conflicts, uninstall, backup, restore, and import/export.
- [ ] Ship production-ready adapters for Claude Code, Codex, OpenCode, Hermes, Devin, Antigravity, and Pi.
- [ ] Detect Agent-side changes every time the desktop application starts.
- [ ] Continue monitoring local Agent configuration changes while the application is running.
- [ ] Ship the `rigdeck` CLI using the same Rust core as the desktop application.
- [ ] Ship a `rigdeck-manager` Skill so an Agent can safely manage RigDeck through the CLI.
- [ ] Support Simplified Chinese and English in the application and documentation.
- [ ] Provide multiple themes and a complete initial brand asset set.
- [ ] Allow new Agent types to be added as adapters without changing core application code.

### Explicit Non-Goals for v1

- [ ] Do not create a SaaS account system or require a cloud backend.
- [ ] Do not treat a shared skills directory as the product's canonical domain model.
- [ ] Do not execute scripts from an untrusted Skill merely to inspect or audit it.
- [ ] Do not use browser automation or private APIs to pretend unsupported cloud Agent capabilities are manageable.
- [ ] Do not silently overwrite user-managed files or unresolved conflicts.
- [ ] Do not enable telemetry by default.
- [ ] Do not make Linux a GA blocker; retain architectural portability and consider Linux after Windows/macOS GA.

## Phase 0 — Charter, Brand, and Repository Bootstrap

### Product and engineering decisions

- [x] Create a new `rigdeck` repository with an MIT license.
- [ ] Write the bilingual PRD, glossary, threat model, contribution guide, release policy, and architecture decision records.
- [ ] Define the primary audience as developers and power users who use multiple local or repository-aware coding agents.
- [ ] Define local-first behavior: no login, no mandatory server, offline management of already installed assets.
- [ ] Record old Registry behaviors only as migration inputs and rejected architecture decisions.
- [ ] Re-run exact-name searches for GitHub, npm, PyPI, crates.io, app stores, trademarks, and relevant domains immediately before public announcement.

### Brand deliverables

- [ ] Finalize the RigDeck wordmark and bilingual slogan.
- [ ] Design a minimal geometric logo: rounded `D`/deck outline with three connected modules representing Skill, Prompt, and MCP.
- [ ] Deliver master SVG, monochrome variants, horizontal lockups, `.ico`, `.icns`, PNG sizes 16–1024, favicon, CLI mark, and GitHub social preview.
- [ ] Define brand colors and design tokens without embedding raw colors in UI components.
- [ ] Provide Porcelain, Obsidian, Aurora, and Follow System themes.

### Acceptance gate

- [x] The new repository has no runtime dependency on `agent-skill-registry` paths, manifests, or scripts.
- [ ] Every required product capability has a measurable acceptance criterion in the PRD.
- [ ] Logo is recognizable at 16×16, works in monochrome, and passes contrast checks on light and dark backgrounds.
- [ ] Name availability evidence is dated and archived; trademark/domain uncertainty is explicitly resolved before public launch.

## Phase 1 — Domain Model and Adapter SDK

### Canonical domain model

- [ ] Implement `Asset` for `skill`, `prompt`, and `mcp_server` kinds.
- [ ] Implement immutable `AssetRevision` with normalized hash, raw hash, source provenance, license, and audit result.
- [ ] Implement `Source` for GitHub, skills.sh, MCP registries, local folders, archives, and configurable private sources.
- [ ] Implement `AgentAdapter`, `AgentInstance`, `Assignment`, `Projection`, `DeploymentPlan`, `DeploymentSnapshot`, `Conflict`, `SecretRef`, and `AuditEvent`.
- [ ] Identify a Skill by source namespace + repository/package + relative path + declared name, not directory name alone.
- [ ] Version every persistent schema and public protocol from the first release.

### Adapter contract

- [ ] Define a versioned `adapter.json` JSON Schema.
- [ ] Include adapter ID, version, platforms, detection rules, asset capabilities, scopes, native formats, codecs, limitations, and official documentation links.
- [ ] Define optional JSON-RPC 2.0 over stdio helpers for behavior that cannot be expressed declaratively.
- [ ] Define methods: `describe`, `detect`, `scan`, `validate_asset`, `render`, `plan_install`, `plan_update`, `plan_remove`, `verify`, and `health`.
- [ ] Prohibit adapters from writing files directly; adapters return projections and operations to the core planner.
- [ ] Require an explicit trust decision before running a third-party helper executable.
- [ ] Provide `rigdeck adapter scaffold`, `validate`, `test`, and `pack` developer commands.
- [ ] Publish an Adapter Contract Test Kit and realistic Windows/macOS fixtures.

### Acceptance gate

- [ ] A mock Agent becomes detectable by installing an adapter package without recompiling or editing Core.
- [ ] Core contains no Agent-name conditional branches.
- [ ] Adapter compatibility, error codes, protocol negotiation, and deprecation behavior are documented and tested.
- [ ] Path traversal, symlink escape, recursive link, malformed manifest, and incompatible protocol versions fail safely.

## Phase 2 — Shared Rust Core, Storage, and Transaction Engine

### Repository structure

- [x] Create a Cargo workspace containing `rigdeck-core`, `rigdeck-store`, `rigdeck-adapter-sdk`, `rigdeck-adapters`, `rigdeck-registry`, `rigdeck-security`, and `rigdeck-cli`.
- [x] Create `apps/desktop` for the React/TypeScript frontend and Tauri shell.
- [x] Keep Tauri IPC handlers thin; all business behavior belongs in shared Rust crates.
- [x] Store the meta Skill under `packages/rigdeck-manager-skill`.

### Local persistence

- [ ] Use SQLite for metadata, inventory, assignments, baselines, conflict records, migrations, and audit events.
- [ ] Use a content-addressed object store for immutable asset revisions and operation backups.
- [ ] Use Windows Credential Manager and macOS Keychain for secrets.
- [ ] Persist only `SecretRef` identifiers in SQLite and exported plans.
- [ ] Implement migration, backup, integrity check, and recovery procedures.
- [ ] Rebuild inventory from Agent-native state if cache/index data is lost.

### Planner and transaction behavior

- [ ] Generate a file-level `DeploymentPlan` before every mutation.
- [ ] Include operation preconditions, input hashes, target paths, rendered diff, compatibility losses, risk level, and rollback location.
- [ ] Invalidate a plan if any source or target hash changes before apply.
- [ ] Apply changes atomically wherever the OS/filesystem permits.
- [ ] Back up every overwritten or removed target before mutation.
- [ ] Roll back both filesystem and database state after injected partial failures.
- [ ] Make repeated plan/apply operations idempotent.
- [ ] Preserve unknown configuration keys, comments, line endings, encoding, and user formatting where the target format permits.

### Acceptance gate

- [ ] Failure injected at every write step restores the exact pre-apply state.
- [ ] Reapplying a successful assignment produces an empty plan.
- [ ] Test secrets do not appear in SQLite, logs, backups, crash reports, or JSON CLI output.
- [ ] Database migration failure leaves the previous database usable.
- [ ] GUI and CLI generate byte-equivalent plans for identical inputs.

## Phase 3 — Initial Agent Adapters

### Shared readiness definition

Every initial adapter must:

- [ ] Detect absence, default installation, alternate path, and multiple profiles/instances.
- [ ] Import existing Skills, prompt/rule files, and MCP configurations where the Agent supports them.
- [ ] Plan and verify install, update, enable/disable, and uninstall operations.
- [ ] Retain native unknown fields and unmanaged user content.
- [ ] Detect external additions, modifications, removals, and renames.
- [ ] Return `unsupported` or `manual_required` with a reason instead of silently dropping unsupported behavior.
- [ ] Pass Windows and macOS golden fixtures and transaction tests.

### Claude Code

- [ ] Support personal and project `.claude/skills` locations.
- [ ] Manage bounded blocks in global/project `CLAUDE.md` without taking ownership of the whole file.
- [ ] Import and project local/remote MCP configurations using supported scopes.
- [ ] Preserve Claude-specific Skill frontmatter without forcing unsupported fields onto other Agents.

### Codex

- [ ] Support global/project Agent Skills locations used by current Codex releases.
- [ ] Manage bounded blocks in global/project `AGENTS.md`.
- [ ] Import and project MCP definitions in `~/.codex/config.toml`, including stdio, Streamable HTTP, bearer-token environment references, and OAuth metadata supported by Codex.
- [ ] Verify against current Codex CLI help and official schemas in CI fixtures.

### OpenCode

- [ ] Support `~/.config/opencode/skills`, `.agents/skills`, and other currently documented discovery paths with explicit precedence.
- [ ] Manage global/project instruction surfaces without overwriting unrelated configuration.
- [ ] Preserve JSONC comments while managing local/remote MCP definitions, enabled state, OAuth, and per-agent permissions.

### Hermes

- [ ] Support `~/.hermes/skills` and configured external skill directories.
- [ ] Model local precedence and the fact that Hermes may edit writable external Skills itself.
- [ ] Manage bounded content in `SOUL.md` and supported context surfaces.
- [ ] Preserve YAML comments and unrelated configuration while managing `mcp_servers`.
- [ ] Treat Hermes agent-created changes as a primary three-way conflict test case.

### Antigravity

- [ ] Support global and workspace Skills under current `.agents`/Gemini configuration paths.
- [ ] Manage global rules and workspace rules according to native activation semantics.
- [ ] Support plugin bundles containing Skills, Rules, MCP definitions, and optional hooks without silently enabling hooks.
- [ ] Preserve backward-compatible legacy paths while writing only the current preferred layout.

### Pi

- [ ] Support `~/.pi/agent/skills`, `~/.agents/skills`, configured resource paths, and project `.pi`/`.agents` locations.
- [ ] Manage global `AGENTS.md` and prompt resources without overwriting unrelated content.
- [ ] Ship an opt-in RigDeck Pi MCP Extension for local/remote MCP connectivity because Pi does not expose an equivalent stable built-in MCP configuration surface.
- [ ] Require approval before installing or enabling the companion extension.

### Devin

- [ ] Manage repository-scoped `.agents/skills` and other officially scanned repository locations.
- [ ] Manage repository `AGENTS.md` through bounded blocks.
- [ ] Detect and describe Devin's repository-scoped limitations rather than inventing a local global-skill directory.
- [ ] Use only documented public APIs for cloud playbook, knowledge, integration, or MCP operations.
- [ ] Where no official write API exists, generate a validated export and a `manual_required` handoff with exact UI instructions; do not use private APIs or browser automation.

### Acceptance gate

- [ ] All seven adapters pass their capability matrices and fixtures.
- [ ] Every advertised supported cell has install/import/drift/update/uninstall evidence.
- [ ] Every unsupported cell has a visible limitation and recovery path.
- [ ] No adapter can mutate files outside paths explicitly included in an approved plan.

## Phase 4 — Skill, Prompt, and MCP Lifecycle

### Skill discovery and lifecycle

- [ ] Implement search/import providers for skills.sh, GitHub repositories/subdirectories, local directories, URLs, and archives.
- [ ] Support configurable private sources without storing credentials in provider URLs.
- [ ] Display source, revision, license, author, update time, file inventory, platform restrictions, and audit findings before install.
- [ ] Support install, update, pin, fork, rename, enable/disable, archive, uninstall, and restore.
- [ ] Do not equate an identical Skill name with identical provenance or content.
- [ ] Cache remote metadata with ETag/conditional requests and handle rate limits explicitly.

### Prompt lifecycle

- [ ] Represent prompts/rules as composable assets with ordering and scope.
- [ ] Project assets into bounded managed blocks with asset ID and revision hash markers.
- [ ] Preserve all user content outside managed blocks.
- [ ] Map conditional activation to native Agent rules where supported.
- [ ] Show a compatibility-loss warning when conditional semantics must be flattened.
- [ ] Remove only the relevant managed block during uninstall.

### MCP lifecycle

- [ ] Normalize stdio and Streamable HTTP server definitions.
- [ ] Model command, args, URL, headers, env references, enabled state, timeouts, OAuth metadata, and tool filters.
- [ ] Consume the Official MCP Registry as a preview metadata source with provider abstraction for compatible downstream/private registries.
- [ ] Distinguish registry presence, namespace verification, package provenance, vulnerability information, and RigDeck static audit results.
- [ ] Provide explicit health probes that never run automatically during passive browsing.
- [ ] Store credentials only in OS keychains and materialize plaintext only after a high-risk confirmation when an Agent cannot reference a secret indirectly.

### Acceptance gate

- [ ] Remote providers being unavailable does not prevent local inventory, uninstall, restore, or conflict resolution.
- [ ] GitHub rate limits produce actionable status and never corrupt cached inventory.
- [ ] Install previews include every target file and compatibility loss.
- [ ] Malicious archives, absolute paths, traversal, and symlink escapes are rejected before extraction or projection.
- [ ] Exported bundles contain placeholders/SecretRefs rather than secret values.

## Phase 5 — Startup Refresh, Watchers, and Conflict Resolution

### Startup refresh

- [ ] Detect all Agent instances on every desktop launch.
- [ ] Scan Skill, prompt, and MCP surfaces before trusting cached clean state.
- [ ] Compute normalized and raw hashes and compare them with the last deployment baseline.
- [ ] Classify `managed_clean`, `managed_modified`, `external_new`, `external_removed`, `source_update_available`, `conflict`, `unsupported`, and `manual_required`.
- [ ] Show a startup refresh summary on Overview.
- [ ] Keep remote catalog refresh asynchronous and non-blocking.
- [ ] Start filesystem watchers after the initial scan and retain a manual full-refresh command as recovery from missed events.

### Conflict taxonomy

- [ ] Same logical asset and same content duplicated.
- [ ] Same declared name with different content or provenance.
- [ ] Managed asset modified on the Agent side.
- [ ] Source update and Agent-side modification since the same baseline.
- [ ] Prompt block edited, duplicated, moved, or structurally damaged.
- [ ] MCP server name/key collision.
- [ ] MCP secret binding missing or invalid.
- [ ] Target capability loss or unsupported field.
- [ ] Directory name/frontmatter name mismatch.
- [ ] Case-only rename, invalid target name, symlink loop, and inaccessible path.
- [ ] Asset removed and then automatically recreated by an Agent.

### Resolution behavior

- [ ] Automatically resolve identical content, one-sided changes, safe path normalization, and non-overlapping configuration keys.
- [ ] Require user review for overlapping content changes or semantic differences.
- [ ] Offer keep RigDeck revision, import Agent revision, keep per-Agent fork, rename and coexist, three-way merge, per-file selection, abandon plan, and restore backup.
- [ ] Never label force overwrite as conflict resolution.
- [ ] Persist the decision, new baseline, and audit event after resolution.

### Acceptance gate

- [ ] Agent-side add/modify/delete/rename scenarios are detected after restart and by watchers.
- [ ] Startup UI is interactive within 1.5 seconds on the reference machine while scanning continues safely.
- [ ] Cold scan of 1,000 typical Skills completes within 3 seconds P95 on the reference SSD machine.
- [ ] Watcher changes appear within 2 seconds under normal local filesystem conditions.
- [ ] Every conflict contains cause, affected projections, risk, and at least one valid next action.
- [ ] Resolving a conflict then refreshing produces a stable clean or intentionally divergent state.

## Phase 6 — Desktop Experience

### Information architecture

- [ ] Build primary navigation: Overview, Library, Agents, Assemble, Conflicts, Activity, Settings.
- [ ] Overview: Agent health, refresh summary, available updates, conflicts, and risk notices.
- [ ] Library: Skill/Prompt/MCP tabs, search, source, compatibility, risk, license, and installed filters.
- [ ] Agents: detection, version, paths, capability matrix, assignments, and health.
- [ ] Assemble: assets, target Agents/scopes, rendered plan, compatibility losses, and confirmation.
- [ ] Conflicts: taxonomy filters, three-way diff, suggested actions, and safe bulk resolution.
- [ ] Activity: operation history, file changes, results, backup, and restore entry points.

### UX, themes, and localization

- [ ] Implement all colors, typography, spacing, radii, and motion through design tokens.
- [ ] Implement Porcelain, Obsidian, Aurora, and system themes.
- [ ] Externalize every user-visible string; forbid untranslated literal strings in CI.
- [ ] Ship Simplified Chinese and English at feature parity.
- [ ] Support full keyboard navigation and visible focus states.
- [ ] Support Windows scaling at 125%, 150%, and 200%, plus macOS Retina.
- [ ] Design explicit empty, loading, offline, rate-limited, permission-denied, unsupported, manual-required, and failure states.
- [ ] Sanitize all rendered Markdown and external metadata.

### Acceptance gate

- [ ] Critical journeys pass desktop E2E tests on Windows and macOS.
- [ ] Chinese and English screenshot regression tests contain no clipping or untranslated placeholders.
- [ ] Critical workflows meet WCAG 2.1 AA contrast and keyboard requirements.
- [ ] Every write action displays target paths, diffs, risks, and rollback availability.
- [ ] Frontend components never edit Agent files or SQLite directly.

## Phase 7 — CLI and Meta-Management Skill

### CLI commands

- [ ] `rigdeck agents detect`
- [ ] `rigdeck refresh`
- [ ] `rigdeck search skill <query>` and `rigdeck search mcp <query>`
- [ ] `rigdeck inspect <asset>` and `rigdeck add <source>`
- [ ] `rigdeck assign <asset> --agent <id> [--scope <scope>]`
- [ ] `rigdeck plan` and `rigdeck apply <plan-id>`
- [ ] `rigdeck status`
- [ ] `rigdeck conflicts list/show/resolve`
- [ ] `rigdeck update`, `remove`, `backup`, `restore`, and `doctor`
- [ ] `rigdeck adapter list/validate/test/scaffold/pack`
- [ ] `rigdeck export` and `import`

### CLI behavior

- [ ] Human-readable tables by default and stable versioned JSON with `--json`.
- [ ] All write commands generate or consume an explicit plan.
- [ ] Non-interactive writes require `--yes --plan <id>`.
- [ ] Use stable exit codes for success, drift, conflict, compatibility failure, invalid plan, permission failure, and internal error.
- [ ] Distribute as a standalone native binary without requiring Node.js.

### `rigdeck-manager` Skill

- [x] Teach Agents to begin with `rigdeck status --json` and `rigdeck refresh` when necessary.
- [x] Use search/inspect before assigning assets.
- [x] Generate and summarize a plan before mutation.
- [x] Require user confirmation before apply, overwrite, update, remove, restore, or extension installation.
- [x] Verify with refresh and doctor after mutation.
- [x] Forbid direct editing of RigDeck SQLite, object storage, and backups.
- [x] Avoid absolute user paths and secrets.
- [ ] Package for all seven initial Agent discovery mechanisms.

### Acceptance gate

- [ ] CLI and Desktop return the same inventory, conflicts, and plan operations for the same fixture.
- [ ] JSON output validates against published schemas.
- [ ] Non-interactive write attempts without a valid confirmation/plan fail closed.
- [ ] The meta Skill is discoverable and behaviorally tested in all seven Agent fixtures.

## Phase 8 — Security, Reliability, and Performance

### Security

- [ ] Complete STRIDE threat modeling for sources, archives, adapters, helper processes, MCP credentials, updaters, and filesystem writes.
- [ ] Sandbox or tightly constrain third-party adapter helpers and disclose requested access.
- [ ] Implement archive bomb limits, maximum file counts/sizes, path normalization, and symlink policy.
- [ ] Detect suspicious Skill content such as credential harvesting, hidden executable payloads, unsafe install instructions, and prompt-injection patterns without claiming perfect safety.
- [ ] Generate SBOMs for desktop and CLI releases.
- [ ] Run Rust and JavaScript dependency audits in CI with documented exception policy.
- [ ] Redact secrets from logs, diagnostics, crash output, plans, and UI copy operations.

### Reliability and performance

- [ ] Add fault injection for process termination, disk full, permission denied, locked files, database corruption, and concurrent Agent writes.
- [ ] Support 10,000 indexed assets without freezing the UI.
- [ ] Deduplicate identical content in the object store.
- [ ] Ensure background work is cancellable and bounded.
- [ ] Provide `rigdeck doctor` checks for database, object store, keychain, adapters, Agent paths, stale plans, and recoverable backups.

### Acceptance gate

- [ ] No unresolved critical/high dependency vulnerability without an approved written exception.
- [ ] Secret scanning finds no test credentials in persistence, logs, export, or crash fixtures.
- [ ] Security fixtures block traversal, symlink escape, malformed archive, and untrusted helper execution.
- [ ] Chaos tests prove rollback or produce a deterministic recovery instruction.
- [ ] Performance budgets are measured in CI on documented reference hardware (Intel i5-12400 / 16GB RAM / NVMe SSD).

## Phase 9 — Packaging, Documentation, and Release

### Packaging and updates

- [ ] Produce Windows EXE/MSI installers and standalone CLI archives.
- [ ] Produce macOS universal or separate architecture DMG packages and standalone CLI archives.
- [ ] Sign Windows artifacts with an appropriate code-signing certificate (blocked: certificate not yet purchased).
- [ ] Sign and notarize macOS artifacts with Developer ID (blocked: Apple Developer Program enrollment pending).
- [ ] Sign update manifests and reject invalid or downgraded updates.
- [ ] Support upgrade, rollback, repair, and clean uninstall without deleting user Agent configuration.

### Documentation

- [ ] Bilingual quickstart, user guide, concepts, security model, troubleshooting, CLI reference, Adapter SDK, and contribution guide.
- [ ] Capability matrix for every initial Agent with official-source links and last-verified versions.
- [ ] Tutorials for importing local Skills, installing from skills.sh/GitHub, configuring MCP secrets, resolving drift, and restoring a backup.
- [ ] Explain unsupported/manual-required behavior, especially for Devin cloud surfaces and optional Pi MCP extension.
- [ ] Provide a one-time importer for the old Registry format without creating an ongoing dependency.

### Release gates

- [ ] Core critical-module test coverage is at least 85%.
- [ ] Adapter contract and golden fixture suites pass 100%.
- [ ] All P0/P1 defects are closed.
- [ ] Windows/macOS clean install, upgrade, rollback, and uninstall matrices pass.
- [ ] Update signature failure tests pass.
- [ ] A new user can complete first Agent detection and one safe Skill assignment within 10 minutes using documentation only.
- [ ] Chinese and English application/documentation releases are synchronized.

## Cross-Cutting Test Matrix

- [ ] Unit tests for hashing, identity, schemas, paths, SecretRef, codecs, and merge rules.
- [ ] Property-based tests for arbitrary paths, render/scan stability, and idempotency.
- [ ] Golden tests for native configuration formats for all seven Agents on Windows/macOS.
- [ ] Integration tests using isolated temporary HOME directories.
- [ ] Transaction tests for install, update, drift, conflict, uninstall, and restore.
- [ ] Desktop E2E tests for all critical human workflows.
- [ ] CLI/Desktop equivalence tests.
- [ ] Localization and screenshot regression tests.
- [ ] Security fixtures and chaos/fault-injection tests.
- [ ] Release artifact installation tests on clean VMs.

## Milestones and Indicative Effort

- [ ] M1 — Product contract, brand, Core model, and Adapter SDK.
- [ ] M2 — Transactional Core plus Claude Code, Codex, and OpenCode adapters.
- [ ] M3 — Hermes, Antigravity, Pi, and Devin adapters.
- [ ] M4 — Discovery, startup refresh, watchers, conflicts, and full uninstall/restore loop.
- [ ] M5 — Desktop UX, CLI, and meta-management Skill.
- [ ] M6 — Security hardening, signed Beta, documentation, and GA.
- Expected effort: approximately 20–26 weeks for two engineers plus part-time design/QA; approximately 8–12 months for one full-time engineer targeting a polished GA.
- Do not label a shortened 10–12 week build as GA; that scope can only be an Alpha with fewer adapters and a reduced lifecycle.

## Final Definition of Done

- [ ] RigDeck is a standalone product and does not require the old Registry to run.
- [ ] Agent types are runtime adapters, not a hardcoded enum in Core.
- [ ] Skill, Prompt, and MCP assets share one lifecycle and one safety model.
- [ ] GUI, CLI, and meta Skill use the same Rust Core and terminology.
- [ ] Every launch performs a trustworthy local refresh.
- [ ] External Agent changes become explainable drift or conflicts, never silent data loss.
- [ ] Every mutation is planned, previewed, backed up, applied, verified, audited, and recoverable.
- [ ] All seven required Agents meet their published capability matrices.
- [ ] Windows/macOS packages are signed, bilingual, documented, and release-tested.
