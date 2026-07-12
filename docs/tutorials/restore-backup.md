# 从备份恢复

## 目标

创建一致性备份，模拟误操作删除文件后从备份恢复，并检查完整性。

## 前置条件

- 已安装 `rigdeck` CLI。
- 已完成至少一次 `apply`，本地有托管资产。
- RigDeck 数据目录可写。

## 步骤

### 1. 创建备份

```bash
rigdeck backup
```

预期输出：

```text
已创建 backup_01...（N 个对象）
```

记录返回的 `backup_id`。

### 2. 模拟误操作

手动删除 Agent 端已安装的 Skill 文件：

```bash
rm -rf ~/.claude/skills/demo-local
```

确认文件已不存在：

```bash
ls ~/.claude/skills/demo-local
```

预期：`No such file or directory`。

### 3. 从备份恢复

```bash
rigdeck restore backup_01... --yes
```

`restore` 在恢复前会自动创建一个 recovery 备份，防止恢复动作本身造成不可逆损失。

预期输出：

```text
已恢复备份 backup_01...
```

### 4. 验证文件回来

```bash
ls ~/.claude/skills/demo-local/SKILL.md
```

预期：文件路径正常输出，文件存在。

### 5. 刷新并检查完整性

```bash
rigdeck refresh --json
echo $?
```

预期：退出码 `0` 或 `10`。若为 `10`（漂移），说明 Agent 端状态与基线仍有差异，按《处理 Agent 端漂移》解决；若为 `0`，状态完全一致。

### 6. 运行完整性诊断

```bash
rigdeck doctor
```

预期输出：

```text
通过    sqlite_integrity    数据库完整性正常
通过    object_store        对象库校验正常
通过    adapters            内置适配器加载正常
通过    paths               数据目录可写
```

退出码 `0` 表示全部通过；`70` 表示存在失败项，按对应 `check.id` 排查。

## 验证

```bash
rigdeck status
```

预期输出中资产数与分配数与备份时一致：

```text
Agent：N
资产：N
分配：N
未解决冲突：0
```

退出码 `0` 表示无未解决冲突。恢复完成后，备份 `backup_01...` 仍可重复使用；恢复时自动创建的 recovery 备份也可用于撤销本次恢复。
