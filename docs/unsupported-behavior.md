# 不支持与人工处理行为说明

RigDeck 的原则：不支持的能力返回明确的 `unsupported` 或 `manual_required`，而不是静默跳过或用浏览器自动化伪装支持。

## Devin

### 仓库 Skill 和 AGENTS.md

Devin 的 Skills 和 `AGENTS.md` 都是仓库作用域的，没有本地全局目录。RigDeck 检测和扫描这些路径：

- `.agents/skills/<name>/SKILL.md`（首选）
- `.github/skills`、`.claude/skills`、`.cursor/skills`、`.codex/skills`、`.cognition/skills`（只读导入）

这些能力完整支持：导入、安装、更新、漂移检测、卸载。

### 云端能力（不支持自动操作）

Devin 的以下云端能力没有公开的写入 API：

| 能力 | RigDeck 行为 |
|------|-------------|
| Playbook | `manual_required`：生成导出文件和操作步骤 |
| Knowledge base | `manual_required`：生成导出文件和操作步骤 |
| Integration | `manual_required`：生成导出文件和操作步骤 |
| 云端 MCP | `manual_required`：生成导出文件和操作步骤 |

RigDeck 不会：
- 调用 Devin 的私有 API
- 使用浏览器自动化操作 Devin 网页
- 伪造这些能力已经成功安装

### 人工交接流程

当操作返回 `manual_required` 时，RigDeck 会：

1. 生成一个经过验证的导出文件（JSON 或 Markdown）
2. 在计划中标注需要人工完成的步骤
3. 显示 Devin UI 中的具体操作路径
4. 等待用户确认完成后更新状态

## Pi

### Skills 和 AGENTS.md

Pi 的 Skills 和 `AGENTS.md` 完整支持：导入、安装、更新、漂移检测、卸载。

Pi 的 frontmatter `name` 允许与父目录不同，RigDeck 不会把这种差异误报为 `declared_name_mismatch`。

### MCP Server（需要人工处理）

Pi 没有与其他 Agent 等价的稳定内置 MCP 配置表面。RigDeck 提供可选的 companion extension 来补充这个能力。

| 场景 | RigDeck 行为 |
|------|-------------|
| 安装 companion extension | `manual_required`：显示代码 hash、路径和权限，等待用户确认 |
| 启用 companion extension | `manual_required`：显示权限清单，等待用户确认 |
| 配置 MCP server | 通过 companion extension 中转 |

### Companion extension 安装流程

1. RigDeck 显示 extension 的代码 hash 和安装路径
2. 用户确认后，RigDeck 执行安装
3. 安装完成后，RigDeck 显示 extension 请求的权限清单
4. 用户确认后，extension 被启用
5. 此后 MCP server 的配置通过 extension 完成

## 通用规则

- `unsupported`：当前 Adapter 声明不支持此能力，计划不会生成相关操作
- `manual_required`：能力理论上可行但没有自动化路径，生成交接文件和步骤
- 两种情况都会在计划预览中显示，用户不会在不知情的情况下遇到缺失
