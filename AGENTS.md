# RigDeck — Agent Working Guide

## Project Overview

RigDeck is a local-first desktop application for managing Agent Skills, prompts/rules, and MCP servers across multiple coding agents. Built with Tauri (Rust core + React frontend).

## Build & Test Commands

```bash
# Build all Rust crates
cargo build --workspace

# Run Rust tests
cargo test --workspace

# Run CLI
cargo run -p rigdeck-cli

# Frontend dev (from apps/desktop)
npm run dev

# Tauri dev (from apps/desktop)
npm run tauri dev

# Lint
cargo clippy --workspace
```

## Architecture Rules

- **Core contains no Agent-name conditional branches.** Agent types are runtime adapters.
- **Adapters never write files directly.** Adapters return projections and operations to the core planner.
- **Every mutation goes through the planner.** Plan → preview → backup → apply → verify → audit.
- **Frontend never touches SQLite or Agent files directly.** All access through Tauri IPC → Rust core.
- **Secrets never appear in SQLite, logs, plans, or JSON output.** Use SecretRef identifiers.

## Code Style

- Rust: follow `rustfmt` defaults, `clippy` clean.
- TypeScript: strict mode, no `any` without explicit comment.
- All user-visible strings externalized to i18n files.

## Key Decisions

See `docs/adr/` for architecture decision records.
See `TODO.md` for the full project plan.
