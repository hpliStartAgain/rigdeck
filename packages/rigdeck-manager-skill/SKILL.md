---
name: rigdeck-manager
description: |
  Meta-management Skill that teaches Agents to safely manage RigDeck through the CLI.
  Agents begin with `rigdeck status --json` and `rigdeck refresh`, use search/inspect
  before assigning assets, generate and summarize a plan before mutation, require user
  confirmation before apply/overwrite/update/remove/restore, and verify with refresh
  and doctor after mutation. Agents must never directly edit RigDeck SQLite, object
  storage, or backups.
---

# rigdeck-manager

## Purpose

Allow any supported coding Agent to safely manage Agent Skills, prompts/rules, and MCP
servers through the `rigdeck` CLI — without directly touching RigDeck's internal state.

## Workflow

1. **Assess current state**: Run `rigdeck status --json` to get inventory, agents, and conflicts.
2. **Refresh if needed**: Run `rigdeck refresh` when the status indicates drift or stale data.
3. **Search before assign**: Use `rigdeck search skill <query>` or `rigdeck search mcp <query>`
   to find assets. Use `rigdeck inspect <asset>` to review source, license, audit findings,
   and compatibility before assigning.
4. **Plan before mutation**: Run `rigdeck plan` to generate a deployment plan. Summarize the
   plan to the user including target paths, diffs, risks, and rollback availability.
5. **Confirm before apply**: Require explicit user confirmation before running
   `rigdeck apply <plan-id> --yes --plan <plan-id>`. Never apply without a valid plan.
6. **Verify after mutation**: Run `rigdeck refresh` and `rigdeck doctor` to verify the
   operation succeeded and no issues were introduced.
7. **Resolve conflicts**: Use `rigdeck conflicts list` to find conflicts, `rigdeck conflicts
   show <id>` to inspect details, and `rigdeck conflicts resolve <id>` to resolve. Never
   force-overwrite as conflict resolution.

## Forbidden Actions

- **Never** directly edit RigDeck's SQLite database, content-addressed object store, or
  backup files.
- **Never** run `rigdeck apply` without user confirmation and a valid plan ID.
- **Never** use absolute user paths or secrets in plans or exports.
- **Never** install or enable the Pi MCP Extension without explicit user approval.
- **Never** use private APIs or browser automation for Devin cloud surfaces.

## CLI Reference

See `rigdeck --help` for the full command list. Key commands:

| Command | Purpose |
|---------|---------|
| `rigdeck status --json` | Get current inventory, agents, conflicts |
| `rigdeck refresh` | Refresh from agent-native state |
| `rigdeck search skill <query>` | Search for skills |
| `rigdeck search mcp <query>` | Search for MCP servers |
| `rigdeck inspect <asset>` | Inspect an asset before install |
| `rigdeck add <source>` | Add an asset from a source |
| `rigdeck assign <asset> --agent <id> [--scope <scope>]` | Assign asset to agent |
| `rigdeck plan` | Generate a deployment plan |
| `rigdeck apply <plan-id> --yes --plan <plan-id>` | Apply a plan (non-interactive) |
| `rigdeck conflicts list` | List active conflicts |
| `rigdeck conflicts show <id>` | Show conflict details |
| `rigdeck conflicts resolve <id>` | Resolve a conflict |
| `rigdeck update` | Update installed assets |
| `rigdeck remove <asset>` | Remove an installed asset |
| `rigdeck backup` | Backup current state |
| `rigdeck restore <backup-id>` | Restore from backup |
| `rigdeck doctor` | Run diagnostic checks |
| `rigdeck export <output>` | Export configuration bundle |
| `rigdeck import <input>` | Import configuration bundle |
