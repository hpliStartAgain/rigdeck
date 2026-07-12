# 处理 Agent 端漂移

## 目标

检测 Agent 手动修改 RigDeck 托管 Skill 产生的漂移，选择保留策略并应用解决。

## 前置条件

- 已按《导入本地 Skill 目录》完成一个 Skill 的导入、分配和应用，状态为 `managed_clean`。
- 知道该 Skill 在 Agent 端的目标路径（`rigdeck assign` 输出的 `target_path`）。

## 步骤

### 1. 模拟 Agent 手动修改

直接编辑 Agent 端 Skill 文件，模拟 Agent 或用户绕过 RigDeck 的修改：

```bash
echo -e '---\nname: demo-local\ndescription: 被手动改过的版本\nlicense: MIT\n---\n# demo-local\n正文被改了。' > ~/.claude/skills/demo-local/SKILL.md
```

### 2. 刷新检测漂移

```bash
rigdeck refresh --json
```

预期：退出码 `10`（漂移）或 `20`（冲突）。输出中对应实例的 `state` 为 `drifted` 或 `conflict`。

```bash
echo $?
```

预期：`10` 或 `20`。

### 3. 列出未解决冲突

```bash
rigdeck conflicts list
```

预期输出（TSV）：

```text
冲突 ID    类型    原因
conflict_01...    content_drift    Agent 端 SKILL.md 与基线不一致
```

### 4. 查看冲突详情

```bash
rigdeck conflicts show conflict_01...
```

预期输出：

```text
冲突 conflict_01...：ContentDrift
原因：Agent 端 SKILL.md 与基线不一致
风险：medium
影响：~/.claude/skills/demo-local/SKILL.md
可选动作：[KeepRigdeckRevision, ImportAgentRevision, KeepAgentFork, ThreeWayMerge, PerFileSelection, AbandonPlan]
```

只从 `可选动作` 列表中选择，不要自创动作。

### 5. 选择保留策略并生成解决计划

本例选择保留 RigDeck 修订（覆盖 Agent 端手动改动）：

```bash
rigdeck conflicts resolve conflict_01... --action keep-rigdeck-revision --yes
```

预期输出末行：

```text
应用前请检查上述路径与风险，再运行：rigdeck apply <plan-id> --yes
```

其他常用策略：

| 动作 | 适用场景 |
|---|---|
| `keep-rigdeck-revision` | 放弃 Agent 端改动，恢复 RigDeck 托管内容 |
| `import-agent-revision` | 把 Agent 端改动反向映射为新修订并采纳 |
| `keep-agent-fork` | 保留 Agent 端改动，将其采纳为该分配的新基线 |
| `three-way-merge` | 双边均有有效修改且可自动合并；重叠时用 `--merged-file` 提交人工正文 |
| `per-file-selection` | 多文件冲突，按路径分别选择，需 `--selections-file` 传 JSON |
| `abandon-plan` | 放弃本次解决尝试，冲突保持未解决 |

### 6. 应用解决计划

```bash
rigdeck apply <plan-id> --plan <plan-id> --yes
```

预期输出：

```text
计划 <plan-id> 已应用，完成 1 个操作
```

## 验证

```bash
rigdeck refresh --json
echo $?
```

预期：退出码 `0`，对应实例状态为 `managed_clean`。

确认 Agent 端文件已恢复为 RigDeck 修订内容：

```bash
grep "被手动改过" ~/.claude/skills/demo-local/SKILL.md
```

预期：无匹配（手动改动已被 RigDeck 修订覆盖）。

同一冲突重复 `resolve` 时，旧的 pending 解决计划会自动标记为 `abandoned`，不会产生悬空计划。
