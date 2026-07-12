---
name: rigdeck-manager
description: 通过 RigDeck CLI 安全管理 Agent Skill、Prompt/Rule 与 MCP Server；先刷新和检查，再生成计划、请求人工确认、应用并验证，禁止直接编辑 RigDeck 内部存储。
license: MIT
---

# RigDeck 管理器

## 目标

通过 `rigdeck` CLI 管理七种 Agent 的能力装配，同时保持计划、预览、备份、应用、验证、审计和恢复闭环。这个 Skill 只调用公开 CLI，绝不直接修改 RigDeck 的 SQLite、对象库或备份。

## 强制工作流

1. **读取状态**：先运行 `rigdeck status --json`。退出码 `20` 表示存在冲突，不等于 CLI 崩溃。
2. **刷新本地事实**：状态过期、准备写入或用户要求核验时运行 `rigdeck refresh --json`。退出码 `10` 表示漂移，`20` 表示冲突。
3. **先搜索和检查**：用 `rigdeck search skill <query> --json` / `rigdeck search mcp <query> --json` 找候选；用 `rigdeck inspect <asset-id> --json` 检查来源、修订、许可证和静态审计。
4. **生成显式计划**：`rigdeck assign <asset-id> --agent <instance-id> --scope <scope> --json` 会返回部署计划。逐项向用户说明目标路径、操作、diff、兼容损失、风险和回滚对象。
5. **在动作发生前确认**：只有用户已经看到计划并明确同意，才运行 `rigdeck apply <plan-id> --plan <plan-id> --yes --json`。两个计划 ID 必须完全相同。
6. **应用后验证**：依次运行 `rigdeck refresh --json` 与 `rigdeck doctor --json`。若出现漂移/冲突，不得声称成功。
7. **冲突处理**：先 `rigdeck conflicts show <id> --json`；只从记录的允许动作中选择，再运行 `rigdeck conflicts resolve <id> --action <action> --yes --json`。强制覆盖不属于冲突解决。

## 高风险动作

- `update --yes` 和 `remove <asset> --yes` 只生成计划；仍需展示并逐个 `apply`。
- `restore <backup-id> --yes` 会先创建 recovery 备份；必须在动作发生前取得用户确认。
- `add <source> --yes`、`import <bundle> --yes` 会写入 RigDeck 本地资产库；先说明来源和预期资产。
- 安装或启用 Pi MCP companion extension 前，必须展示包 hash、源码路径、依赖、目标路径和权限，并取得单独确认。
- Devin 云端能力只使用官方公开 API；没有公开写接口时生成 `manual_required` 交接，不使用浏览器自动化或私有 API。

## 禁止事项

- 不直接编辑 RigDeck SQLite、加密对象库、备份或 audit 文件。
- 不把 secret、token、Cookie、认证 URL 或绝对用户路径复制到计划、日志、导出包或对话摘要。
- 不在缺少 `--yes --plan <id>` 时尝试机器化应用。
- 不把退出码 `10`（漂移）或 `20`（冲突）吞掉并继续写入。
- 不执行未受信任 Skill 中的脚本来完成检查。

## 稳定退出码

| 退出码 | 含义 | 下一步 |
|---:|---|---|
| 0 | 成功 | 继续验证 |
| 10 | 漂移 | 查看 refresh 结果 |
| 20 | 冲突 | 进入 conflicts 流程 |
| 30 | 兼容失败/人工处理 | 展示限制与恢复路径 |
| 40 | 无效计划/确认不足 | 重新生成计划 |
| 50 | 权限/钥匙串失败 | 修复权限后重试 |
| 70 | 内部错误 | 运行 doctor 并保留脱敏诊断 |

## 常用命令

| 命令 | 作用 |
|---|---|
| `rigdeck agents detect --json` | 检测全部 Agent 实例 |
| `rigdeck refresh --json` | 重建本地库存并检查漂移 |
| `rigdeck search skill <query> --json` | 搜索 Skill |
| `rigdeck search mcp <query> --json` | 搜索 MCP metadata |
| `rigdeck inspect <asset-id> --json` | 检查当前修订 |
| `rigdeck assign <asset-id> --agent <id> --scope <scope> --json` | 生成安装/更新计划 |
| `rigdeck apply <id> --plan <id> --yes --json` | 应用已确认计划 |
| `rigdeck conflicts list --json` | 列出未解决冲突 |
| `rigdeck backup --json` | 创建一致性备份 |
| `rigdeck doctor --json` | 运行诊断 |

任何帮助文本与本 Skill 不一致时，以当前 `rigdeck --help` 和版本化 JSON schema 为准，并停止高风险写入直到合同重新核对。
