# ADR-0002: Technology Stack

- **Status**: Accepted
- **Date**: 2026-07-11

## Context

RigDeck needs a cross-platform desktop stack that is lightweight, local-first, and capable of handling file I/O, SQLite, content-addressed storage, filesystem watching, and a polished multi-theme UI.

## Decision

| Layer | Technology | Version |
|-------|-----------|---------|
| Desktop shell | Tauri | 2.x |
| Frontend | React + TypeScript | React 18, TS 5.5 |
| Build tool | Vite | 5.x |
| UI components | shadcn/ui + Tailwind CSS | Tailwind 3.4 |
| State management | Zustand | 4.x |
| Core / CLI | Rust | 2021 edition, MSRV 1.75 |
| SQLite | rusqlite + refinery | rusqlite 0.31, refinery 0.8 |
| Content hash | Blake3 | 1.5 |
| CLI framework | clap | 4.x (derive) |
| File watcher | notify | 6.x |
| Merge algorithm | diff3 | standard three-way merge |
| i18n | i18next + react-i18next | 23.x / 15.x |

## Rationale

- **Tauri over Electron**: 3-10MB vs 80MB+, lower memory, system WebView.
- **shadcn/ui + Tailwind**: Design-token driven, multi-theme support via CSS variables, high customizability.
- **Zustand over Redux**: Lightweight, no boilerplate, TypeScript-friendly for a medium-complexity app.
- **rusqlite + refinery**: Synchronous, no async overhead, suitable for local tools; refinery for migrations.
- **Blake3 over SHA-256**: 3-5x faster for content-addressed storage; modern design.
- **clap v4**: Most mature Rust CLI framework, derive macros, subcommand support.
- **notify**: Cross-platform file watching, mature Rust ecosystem.
- **diff3**: Standard three-way merge algorithm, same as Git, proven reliability.

## Consequences

- Rust core is shared between CLI and desktop — no logic duplication.
- Frontend communicates with core exclusively through Tauri IPC.
- All business logic lives in Rust crates; Tauri handlers are thin.
