# Cross-Cutting Test Fixtures

This directory contains shared test fixtures used across multiple test suites:

- **Golden fixtures**: Native configuration formats for all seven agents on Windows/macOS.
- **Integration fixtures**: Isolated temporary HOME directories for each agent.
- **Transaction fixtures**: Install, update, drift, conflict, uninstall, and restore scenarios.
- **Security fixtures**: Path traversal, symlink escape, malformed archives, untrusted helpers.
- **Localization fixtures**: Screenshot regression test data for Chinese and English.
- **Release fixtures**: Clean VM installation test artifacts.

## Fixture Organization

```
tests/
├── fixtures/
│   ├── agents/           # Per-agent file layout fixtures
│   │   ├── claude-code/
│   │   ├── codex/
│   │   ├── opencode/
│   │   ├── hermes/
│   │   ├── antigravity/
│   │   ├── pi/
│   │   └── devin/
│   ├── archives/         # Test archives (safe + malicious samples)
│   ├── skills/           # Sample skills for install/update/conflict tests
│   └── configs/          # Sample MCP/prompt configurations
├── golden/               # Golden expected outputs for scan/render/plan
└── chaos/                # Fault injection scenarios
```
