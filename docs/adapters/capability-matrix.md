# 初始 Agent 适配器能力矩阵

最后核验：2026-07-12  
适配器协议：RigDeck Adapter Protocol v1

本页是七个内置适配器的公开能力合同。`完整` 表示 RigDeck 可以在本地完成导入、规划、安装、更新、漂移检测和精确卸载；`受限` 表示原生语义不能完整映射，计划必须展示兼容损失；`人工` 表示只生成经过验证的交接文件和明确步骤，不调用私有 API，也不使用浏览器自动化伪装成支持。

## 总览

| Agent | Skills | Prompt / Rules | MCP | 主要保真策略 |
|---|---|---|---|---|
| Claude Code | 完整：个人、项目 | 完整：`CLAUDE.md` 托管块 | 完整：local/project/user scope | 保留 Skill frontmatter、共享文件外部内容和 MCP 未知字段 |
| Codex | 完整：`.agents/skills`；旧 `.codex/skills` 只读兼容 | 完整：`AGENTS.md` 托管块 | 完整：`~/.codex/config.toml` | 写入当前首选路径；TOML 保留注释和未知键 |
| OpenCode | 完整：原生及兼容发现路径 | 完整：全局/项目指令 | 完整：本地/远程 | JSONC 定点补丁，保留注释、OAuth 和权限字段 |
| Hermes | 完整：主目录；配置外部目录只读扫描/监听 | 完整：`SOUL.md`/项目上下文托管块 | 完整：`config.yaml` | YAML 定点补丁；外部目录不作为默认安装目标，其变化进入 drift 流程 |
| Antigravity | 完整：全局、工作区、旧路径导入；Plugin bundle 尚未自动安装 | 完整：全局/工作区 Rules，条件语义不等价时告警 | 完整：普通 MCP；Plugin MCP 尚未自动安装 | Hooks 永不静默启用；Plugin bundle 当前必须人工交接 |
| Pi | 完整：全局、项目；配置资源路径只读扫描/监听 | 完整：`AGENTS.md`、prompt resources | 人工：需可选 companion extension | 扩展安装/启用属于高风险操作，必须单独确认 |
| Devin | 完整：仓库扫描路径 | 完整：仓库 `AGENTS.md` 托管块 | 人工：仅公开接口或交接文件 | 不发明本地全局目录，不调用私有云端接口 |

## 原生表面与优先级

### Claude Code

- Skills：`~/.claude/skills/<name>/SKILL.md`、`<project>/.claude/skills/<name>/SKILL.md`。
- 指令：`~/.claude/CLAUDE.md` 与项目 `CLAUDE.md`，RigDeck 只拥有带资产 ID 和修订 hash 的块。
- MCP：读取并投影官方支持的 local、project、user scope；秘密值只以 `SecretRef` 进入计划。
- 官方依据：[Claude Code 目录结构](https://code.claude.com/docs/en/claude-directory)、[Skills](https://code.claude.com/docs/en/skills)、[MCP](https://code.claude.com/docs/en/mcp)。

### Codex

- 当前首选 Skill 路径：项目从当前目录向仓库根逐层扫描 `.agents/skills`；全局为 `$HOME/.agents/skills`。
- 旧 `.codex/skills` 仅用于导入和迁移，不作为新写入目标。
- 指令遵循 `AGENTS.override.md`、`AGENTS.md` 和目录向下覆盖规则；RigDeck 不取得整文件所有权。
- MCP 管理 `mcp_servers.<id>`，覆盖 stdio、Streamable HTTP、bearer-token 环境引用、headers、OAuth、超时和工具过滤。
- 官方依据：[Agent Skills](https://developers.openai.com/codex/skills)、[AGENTS.md](https://developers.openai.com/codex/guides/agents-md)、[配置参考](https://developers.openai.com/codex/config-reference)、[MCP](https://developers.openai.com/codex/mcp)。

### OpenCode

- Skills：`~/.config/opencode/skills`、项目 `.opencode/skills`、`.agents/skills`，并按官方规则识别 Claude 兼容路径。
- 指令：项目 `AGENTS.md`、全局 `~/.config/opencode/AGENTS.md`；只管理边界块。
- MCP：对 `opencode.json` / `opencode.jsonc` 做结构化条目补丁，保留注释、未知配置、enabled、OAuth 和权限设置。
- 官方依据：[Skills](https://opencode.ai/docs/skills/)、[Rules](https://opencode.ai/docs/rules/)、[Config](https://opencode.ai/docs/config/)、[MCP servers](https://opencode.ai/docs/mcp-servers/)。

### Hermes

- Skills：`~/.hermes/skills` 是默认事实源；`config.yaml` 的 `skills.external_dirs` 会展开 `~` 与 `${VAR}`、规范化为真实路径，并加入扫描和 watcher。不存在的可选目录按 Hermes 原生语义跳过。
- 外部 Skill surface 为只读发现表面：新安装仍写主目录。Hermes 若原地修改外部可写 Skill，刷新会产生 drift；在 RigDeck 提供逐路径重新绑定前，不把共享目录误称为可自动写入目标。
- 指令：全局 `SOUL.md` 与项目 `.hermes.md` 使用边界块。
- MCP：管理 `~/.hermes/config.yaml` 的 `mcp_servers`，保留 YAML 注释与无关字段。
- Hermes 自身可能修改可写 Skill；RigDeck 把这种修改视作一等三方冲突，不自动覆盖。
- 官方依据：[Hermes Agent 仓库](https://github.com/NousResearch/hermes-agent)、[Skills 与 external_dirs](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/skills.md)。

### Antigravity

- Skills：全局 `~/.gemini/config/skills`，工作区 `.agents/skills`；旧 `.agent/skills` 只读兼容。
- Rules：全局 `~/.gemini/GEMINI.md`，工作区 `.agents/rules`；保留 Manual、Always On、Model Decision、Glob 激活语义。
- Plugins：官方表面是工作区 `.agents/plugins` / `_agents/plugins` 与全局 `~/.gemini/config/plugins`；bundle 可含 Skills、Rules、`mcp_config.json` 和 `hooks.json`。当前 Adapter 只自动处理工作区普通资产投影，完整 Plugin bundle 安装返回 `manual_required`，不会伪装成已支持。
- RigDeck 当前不会执行或启用 Plugin Hook；后续自动化必须先引入独立的 Hook 信任决定、代码 hash 和高风险确认。
- 官方依据：[Skills](https://antigravity.google/docs/skills)、[Rules](https://antigravity.google/docs/ide-rules)、[Plugins](https://antigravity.google/docs/plugins)、[MCP](https://antigravity.google/docs/mcp)。

### Pi

- Skills：`~/.pi/agent/skills`、`~/.agents/skills`、项目 `.pi/skills`、`.agents/skills`。全局与项目 `settings.json.skills` 的相对路径、`~`、glob、排除及强制包含会作为只读发现 surface；相对路径分别以 `~/.pi/agent` 和项目 `.pi` 为基准。
- Pi 官方允许 frontmatter `name` 与父目录不同，Adapter 不会把这种差异误报为 `declared_name_mismatch`；其他遵循 Agent Skills 标准的 Adapter 仍会报告。
- 指令和 Prompt：`~/.pi/agent/AGENTS.md`、目录链 `AGENTS.md`、`~/.pi/agent/prompts` 与 `.pi/prompts`。
- Pi 没有与其他 Agent 等价、稳定的内置 MCP 配置表面；RigDeck 提供可选 companion extension。安装和启用扩展前必须显示代码 hash、路径和权限并取得确认。
- 官方依据：[Pi Skills](https://pi.dev/docs/latest/skills)、[Settings](https://pi.dev/docs/latest/settings)、[Security / Project Trust](https://pi.dev/docs/latest/security)、[Extensions](https://pi.dev/docs/latest/extensions)。

### Devin

- Skills：首选 `.agents/skills/<name>/SKILL.md`，并导入官方扫描的 `.github/skills`、`.claude/skills`、`.cursor/skills`、`.codex/skills`、`.cognition/skills`。
- 指令：仓库任意层级的 `AGENTS.md`；RigDeck 只管理边界块。
- 不声明本地全局 Skill 目录。云端 Playbook、Knowledge、Integration 或 MCP 只有在官方公开写 API 存在且用户明确授权时才可调用；否则生成导出文件和 `manual_required` 步骤。
- 官方依据：[Skills](https://docs.devin.ai/product-guides/skills)、[AGENTS.md](https://docs.devin.ai/onboard-devin/agents-md)、[API 概览](https://docs.devin.ai/api-reference/overview)。

## 通用安全合同

1. Adapter 只返回 `Projection` 和操作意图，不直接写 Agent 文件。
2. 每个目标必须位于检测实例明确列出的 surface 根目录内，目标模板拒绝绝对路径、`..` 和未知变量。
3. 共享文本使用唯一边界块；卸载只删除对应资产块。
4. JSONC、TOML、YAML 只替换所属结构化条目；无法无损解析时停止并报告冲突。
5. 旧路径可以导入但不能成为新写入目标，除非官方仍把它列为首选路径。
6. 不支持的能力必须返回稳定的 `unsupported` 或 `manual_required` 原因与恢复路径。
7. Hook、helper、Pi companion extension 和任何可执行载荷都必须先通过显式信任决策。
