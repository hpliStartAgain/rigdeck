# 导入本地 Skill 目录

## 目标

把一个本地 Skill 目录导入 RigDeck 资产库，分配到 Claude Code 并安装到磁盘。

## 前置条件

- 已安装 `rigdeck` CLI 并在 PATH 中可用。
- 已检测到至少一个 Claude Code 实例（运行 `rigdeck agents detect` 确认）。
- 拥有可写的测试目录。

## 步骤

### 1. 创建测试 Skill 目录

```bash
mkdir -p ~/skills/demo-local
```

### 2. 写 SKILL.md frontmatter

把以下内容写入 `~/skills/demo-local/SKILL.md`。`name` 必须与目录名一致，否则除 Pi 外的 Agent 会报 `declared_name_mismatch`。

```markdown
---
name: demo-local
description: 用于验证 RigDeck 本地导入流程的示例 Skill。
license: MIT
---

# demo-local

这是一个示例 Skill 正文。
```

### 3. 检测 Agent 实例

```bash
rigdeck agents detect --json
```

预期输出（节选）：

```json
{
  "schema_version": 1,
  "ok": true,
  "data": {
    "adapters": [{ "adapter_id": "claude-code", "instance_count": 1 }]
  }
}
```

记下 `data.instances` 中 Claude Code 实例的 ID，后续 `--agent` 使用。

### 4. 导入 Skill

```bash
rigdeck add ~/skills/demo-local --yes
```

预期输出：

```text
已导入 demo-local（资产 asset_01...，修订 rev_01...）
```

### 5. 查看导入结果

```bash
rigdeck inspect asset_01...
```

预期输出：

```text
资产 ID：asset_01...
名称：demo-local
类型：Skill
修订：rev_01...
来源：local-folder
许可证：MIT
审计发现：0 项
```

确认审计发现项为 0 或仅为低风险提示；出现 high 级别 finding 时先排查 Skill 内容，不要继续分配。

### 6. 分配到 Claude Code

```bash
rigdeck assign asset_01... --agent <claude-code-instance-id> --scope global
```

预期输出末行：

```text
应用前请检查上述路径与风险，再运行：rigdeck apply <plan-id> --yes
```

记录返回的 `计划 ID`。

### 7. 应用计划

交互式终端：

```bash
rigdeck apply <plan-id> --yes
```

机器调用或非交互环境必须同时传 `--plan`：

```bash
rigdeck apply <plan-id> --plan <plan-id> --yes --json
```

预期输出：

```text
计划 <plan-id> 已应用，完成 N 个操作
```

## 验证

确认 Claude Code 全局 Skill 目录下出现 `demo-local/SKILL.md`：

```bash
ls ~/.claude/skills/demo-local/SKILL.md
```

运行 `rigdeck refresh --json`，对应实例的漂移状态应为 `managed_clean`，退出码 `0`。
