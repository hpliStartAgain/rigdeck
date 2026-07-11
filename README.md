# RigDeck

**One deck. Every agent, perfectly equipped.**
**一处装配，让每个 Agent 各就其位。**

RigDeck is a local-first desktop application for managing Agent Skills, prompts/rules, and MCP servers across multiple coding agents (Claude Code, Codex, OpenCode, Hermes, Devin, Antigravity, Pi).

## Features

- **Unified domain model** — Skills, prompts, and MCP servers share one lifecycle and one safety model.
- **Adapter architecture** — New agent types are added as runtime adapters without recompiling core.
- **Transactional operations** — Every mutation is planned, previewed, backed up, applied, verified, audited, and recoverable.
- **Drift detection** — Detects agent-side changes on startup and via filesystem watchers.
- **Conflict resolution** — Three-way merge with explicit conflict taxonomy and safe resolution options.
- **Local-first** — No login, no mandatory server, offline management of installed assets.
- **Bilingual** — Simplified Chinese and English at feature parity.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop shell | Tauri |
| Frontend | React + TypeScript + Vite |
| UI components | shadcn/ui + Tailwind CSS |
| State management | Zustand |
| Core / CLI | Rust |
| Storage | SQLite (rusqlite + refinery) + content-addressed store (Blake3) |
| CLI framework | clap v4 |
| File watcher | notify |
| Merge algorithm | diff3 three-way merge |

## Repository Structure

```
rigdeck/
├── crates/
│   ├── rigdeck-core/          # Domain model, planner, transaction engine
│   ├── rigdeck-store/         # SQLite + content-addressed object store
│   ├── rigdeck-adapter-sdk/   # Adapter contract, JSON Schema, test kit
│   ├── rigdeck-adapters/      # Agent adapters (Claude Code, Codex, ...)
│   ├── rigdeck-registry/      # Source providers (GitHub, skills.sh, MCP registry)
│   ├── rigdeck-security/      # Audit, sandbox, secret ref, archive safety
│   └── rigdeck-cli/           # CLI binary
├── apps/
│   └── desktop/               # Tauri + React + shadcn/ui frontend
├── packages/
│   └── rigdeck-manager-skill/ # Meta-management Skill for agents
├── docs/
│   ├── adr/                   # Architecture Decision Records
│   ├── prd/                   # Product Requirements Document
│   └── brand/                 # Brand assets and guidelines
└── tests/                     # Cross-cutting test fixtures
```

## License

MIT
