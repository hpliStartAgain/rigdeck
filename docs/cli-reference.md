# RigDeck CLI 参考

`rigdeck` 是 RigDeck 的命令行入口，统一管理多个 AI 编码 Agent 的 Skills、Prompts/Rules 和 MCP servers。所有变更经 planner 走「计划 → 预览 → 备份 → 应用 → 验证 → 审计」，secrets 永不出现在 SQLite、日志、计划或 JSON 输出中。

## 全局选项

| 选项 | 说明 |
| --- | --- |
| `--json` | 以 JSON 输出，适合脚本消费 |
| `--data-dir <DIR>` | 覆盖本地库存与对象库根目录 |
| `--project-root <DIR>` | 覆盖当前项目根，用于 Agent 实例作用域解析 |

---

## 检测与刷新

### rigdeck agents detect

检测本机已安装的 Agent 实例。

| 参数 | 说明 |
| --- | --- |
| `--json` | 输出结构化列表 |

```bash
rigdeck agents detect --json
```

```json
[
  {"id": "claude-code", "kind": "claude-code", "path": "C:\\Users\\me\\.claude"},
  {"id": "cursor", "kind": "cursor", "path": "C:\\Users\\me\\.cursor"}
]
```

### rigdeck refresh

从 Agent 原生状态刷新本地库存，重建投影与漂移记录。

```bash
rigdeck refresh
```

```
已刷新 2 个 Agent 实例，新增 14 个修订，检测到 3 处漂移。
```

---

## 资产管理

### rigdeck search skill

按关键字搜索 Skill 资产。

| 参数 | 说明 |
| --- | --- |
| `<query>` | 搜索关键字 |
| `--limit N` | 最多返回条数 |

```bash
rigdeck search skill "git commit" --limit 5
```

### rigdeck search mcp

按关键字搜索 MCP server 资产。

| 参数 | 说明 |
| --- | --- |
| `<query>` | 搜索关键字 |
| `--limit N` | 最多返回条数 |

```bash
rigdeck search mcp filesystem --limit 5
```

### rigdeck inspect

查看某个资产的详情，含修订、来源、许可证与兼容损失。

| 参数 | 说明 |
| --- | --- |
| `<asset-id>` | 资产 ID |

```bash
rigdeck inspect skill:git-commit-conventional
```

### rigdeck add

从来源导入资产到本地库存。

| 参数 | 说明 |
| --- | --- |
| `<source>` | 来源（GitHub、URL、本地目录、归档、MCP Registry 等） |
| `--kind skill\|prompt\|mcp` | 资产类型 |
| `--name NAME` | 覆盖资产名 |
| `--scope SCOPE` | 默认作用域 |

```bash
rigdeck add https://github.com/foo/bar --kind skill --name git-commit-conventional
```

```
已导入 skill:git-commit-conventional@rev:7f3a…
```

### rigdeck remove

为资产生成卸载计划（不直接删除文件）。

| 参数 | 说明 |
| --- | --- |
| `<asset>` | 资产 ID |

```bash
rigdeck remove skill:git-commit-conventional
```

### rigdeck pin / unpin / archive / restore-asset

资产生命周期管理。

| 命令 | 说明 |
| --- | --- |
| `rigdeck pin <asset>` | 锁定资产，禁止被刷新覆盖 |
| `rigdeck unpin <asset>` | 解除锁定 |
| `rigdeck archive <asset>` | 归档，从活动视图移除 |
| `rigdeck restore-asset <asset>` | 从归档恢复 |

```bash
rigdeck pin skill:git-commit-conventional
rigdeck archive prompt:legacy-rules
rigdeck restore-asset prompt:legacy-rules
```

---

## 分配

### rigdeck assign

将资产绑定到 Agent 实例与作用域，生成安装计划。

| 参数 | 说明 |
| --- | --- |
| `<asset>` | 资产 ID |
| `--agent <id>` | 目标 Agent 实例 |
| `--scope SCOPE` | 作用域（user / project 等） |

```bash
rigdeck assign skill:git-commit-conventional --agent claude-code --scope user
```

```
已生成计划 plan:01HQ…（pending）
```

### rigdeck assignments

列出或切换分配。

| 参数 | 说明 |
| --- | --- |
| `[--asset ID]` | 只列某资产的分配 |
| `--enable` | 启用匹配的分配 |
| `--disable` | 停用匹配的分配 |

```bash
rigdeck assignments --asset skill:git-commit-conventional
rigdeck assignments --asset skill:git-commit-conventional --disable
```

---

## 计划

### rigdeck plan

列出部署计划。

| 参数 | 说明 |
| --- | --- |
| `--status pending\|applied\|abandoned` | 按状态过滤 |

```bash
rigdeck plan --status pending
```

### rigdeck apply

应用部署计划，执行预览 → 备份 → 写入 → 验证 → 审计。

| 参数 | 说明 |
| --- | --- |
| `<plan-id>` | 计划 ID（位置参数） |
| `--plan <plan-id>` | 计划 ID（选项形式，二选一即可） |
| `--yes` | 跳过交互确认 |

```bash
rigdeck apply 01HQ… --yes
```

```
已应用 plan:01HQ…：写入 3 个文件，备份至 snap:9b2c…
```

---

## 冲突

### rigdeck conflicts list

列出待处理冲突。

```bash
rigdeck conflicts list
```

### rigdeck conflicts show

查看某冲突的语义差异与候选解决方案。

| 参数 | 说明 |
| --- | --- |
| `<conflict-id>` | 冲突 ID |

```bash
rigdeck conflicts show conflict:ab12
```

### rigdeck conflicts resolve

解决冲突，记录决策。

| 参数 | 说明 |
| --- | --- |
| `<conflict-id>` | 冲突 ID |
| `--strategy <name>` | 解决策略（keep / merge / override / manual） |

```bash
rigdeck conflicts resolve conflict:ab12 --strategy keep
```

---

## 维护

### rigdeck status

总览当前库存、Agent 实例、待办计划与漂移。

```bash
rigdeck status
```

### rigdeck update

检查资产来源是否有新版本，提示可刷新的修订。

```bash
rigdeck update
```

### rigdeck doctor

诊断本地环境、依赖、适配器与库存一致性。

```bash
rigdeck doctor
```

### rigdeck backup

将本地库存与对象库打包为备份。

```bash
rigdeck backup
```

### rigdeck restore

从备份恢复本地库存。

| 参数 | 说明 |
| --- | --- |
| `<backup-id>` | 备份 ID 或路径 |

```bash
rigdeck restore snap:9b2c…
```

---

## Adapter 开发

### rigdeck adapter list

列出已注册的适配器及版本。

```bash
rigdeck adapter list
```

### rigdeck adapter validate

校验适配器清单与契约。

| 参数 | 说明 |
| --- | --- |
| `<path>` | 适配器目录 |

```bash
rigdeck adapter validate ./adapters/my-agent
```

### rigdeck adapter test

对适配器跑检测、扫描、编解码、投影与验证用例。

| 参数 | 说明 |
| --- | --- |
| `<path>` | 适配器目录 |

```bash
rigdeck adapter test ./adapters/my-agent
```

### rigdeck adapter scaffold

按模板生成新适配器骨架。

| 参数 | 说明 |
| --- | --- |
| `--name <name>` | 适配器名 |
| `--kind <kind>` | Agent 类型标识 |

```bash
rigdeck adapter scaffold --name my-agent --kind my-agent
```

### rigdeck adapter pack

将适配器打包为可分发的归档。

| 参数 | 说明 |
| --- | --- |
| `<path>` | 适配器目录 |

```bash
rigdeck adapter pack ./adapters/my-agent
```

---

## Secret

Secret 仅以 `SecretRef` 标识符形式存在，明文凭据存于系统钥匙串，永不出现在 SQLite、日志、计划或 `--json` 输出中。

### rigdeck secret set

保存 secret 到系统钥匙串。

| 参数 | 说明 |
| --- | --- |
| `<reference>` | SecretRef 标识符 |
| `--stdin` | 从标准输入读取明文 |
| `--yes` | 跳过覆盖确认 |

```bash
echo -n "sk-xxxx" | rigdeck secret set mcp:github:token --stdin --yes
```

### rigdeck secret check

检查 secret 是否存在（不返回明文）。

| 参数 | 说明 |
| --- | --- |
| `<reference>` | SecretRef 标识符 |

```bash
rigdeck secret check mcp:github:token
```

```
存在
```

### rigdeck secret delete

删除 secret。

| 参数 | 说明 |
| --- | --- |
| `<reference>` | SecretRef 标识符 |
| `--yes` | 跳过确认 |

```bash
rigdeck secret delete mcp:github:token --yes
```

### rigdeck secret list

列出所有已注册的 SecretRef（不含明文）。

```bash
rigdeck secret list
```

```
mcp:github:token
mcp:linear:token
```

---

## 导入导出

### rigdeck export

导出本地库存与分配为可移植包。

| 参数 | 说明 |
| --- | --- |
| `[--out <path>]` | 输出路径，默认当前目录 |

```bash
rigdeck export --out ./rigdeck-bundle.zip
```

### rigdeck import

从导出包导入资产与分配。

| 参数 | 说明 |
| --- | --- |
| `<path>` | 导出包路径 |
| `--yes` | 跳过冲突确认 |

```bash
rigdeck import ./rigdeck-bundle.zip --yes
```
