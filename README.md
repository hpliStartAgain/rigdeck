# RigDeck

**一处装配，让每个 Agent 各就其位。**  
**One deck. Every agent, perfectly equipped.**

RigDeck 是一个本地优先的 Windows/macOS 桌面应用与独立 CLI，用统一的安全模型管理多个编码 Agent 的 Skill、Prompt/Rule 和 MCP Server。无需登录或云端后端；离线时仍可盘点、卸载、恢复和解决本地冲突。

> English summary: RigDeck is a local-first desktop app and native CLI for managing Skills, prompts/rules, and MCP servers across Claude Code, Codex, OpenCode, Hermes, Devin, Antigravity, and Pi.

## 核心能力

- **统一资产模型**：Skill、Prompt 与 MCP Server 共享来源、修订、分配、计划、冲突、备份和审计生命周期。
- **运行时适配器**：Agent 差异位于 Adapter SDK；Core 不包含 Agent 名称分支。
- **事务式变更**：所有写入先生成文件级计划，经过预览、备份、原子应用、验证和审计，失败时恢复原状态。
- **漂移检测**：桌面启动时全量扫描，运行中监听 Agent 原生配置变化。
- **显式冲突处理**：基于上次基线、RigDeck 修订和 Agent 当前状态执行三方比较，不静默覆盖。
- **双语与多主题**：简体中文/英文功能对等，提供 Porcelain、Obsidian、Aurora 和跟随系统主题。

## 仓库结构

```text
crates/rigdeck-core          领域模型、规划器、事务引擎
crates/rigdeck-store         SQLite 与内容寻址对象存储
crates/rigdeck-adapter-sdk   版本化适配器契约和测试工具包
crates/rigdeck-adapters      七个首发 Agent 适配器
crates/rigdeck-registry      GitHub、skills.sh、MCP Registry 等来源
crates/rigdeck-security      静态审计、路径/归档安全、SecretRef
crates/rigdeck-cli           独立 rigdeck 命令行程序
apps/desktop                 Tauri + React 桌面应用
packages/rigdeck-manager-skill 供 Agent 安全调用 CLI 的 meta Skill
```

## 开发环境

- Rust 1.88 或更高版本，组件：`rustfmt`、`clippy`
- Node.js 20 LTS 与 npm
- Windows：Microsoft C++ Build Tools、WebView2
- macOS：Xcode Command Line Tools

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

Set-Location apps/desktop
npm ci
npm run build
```

## 文档

| 文档 | 内容 |
|------|------|
| [快速上手](docs/quickstart.md) | 5 分钟从构建到安装第一个 Skill |
| [用户指南](docs/user-guide.md) | 桌面端和 CLI 的完整使用说明 |
| [CLI 参考](docs/cli-reference.md) | 全部命令的参数和示例 |
| [核心概念](docs/concepts.md) | 资产、修订、计划、冲突、漂移等 |
| [教程](docs/tutorials/) | 导入 Skill、配置 MCP secret、处理漂移、恢复备份 |
| [故障排查](docs/troubleshooting.md) | 常见问题的症状和解决方法 |
| [Adapter SDK](docs/adapter-sdk.md) | 开发新 Agent 适配器 |
| [不支持的行为](docs/unsupported-behavior.md) | Devin/Pi 的 manual-required 说明 |
| [能力矩阵](docs/adapters/capability-matrix.md) | 七个 Agent 的支持程度 |
| [贡献指南](docs/contributing.md) | 开发环境、代码规范、提交流程 |
| [PRD](docs/prd/README.md) | 产品需求文档 |
| [ADR](docs/adr/README.md) | 架构决策记录 |
| [威胁模型](docs/security/threat-model.md) | 安全边界和威胁分析 |

完整产品范围见 [TODO](TODO.md)。

## 当前状态

项目处于完整 GA 范围的实现阶段。未取得 Windows/macOS 签名凭据、未完成跨平台安装矩阵前，不会把构建标记为 GA。

## 许可证

[MIT](LICENSE)

