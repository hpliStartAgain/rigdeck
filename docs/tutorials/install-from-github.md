# 从 GitHub 安装 Skill

## 目标

从 GitHub 仓库搜索、检查并安装一个 Skill 到目标 Agent。

## 前置条件

- 已安装 `rigdeck` CLI。
- 已检测到目标 Agent 实例（运行 `rigdeck agents detect`）。
- 网络可访问 skills.sh 与 github.com。

## 支持的 URL 格式

`rigdeck add` 接受以下 Skill 来源：

| 格式 | 说明 |
|---|---|
| `https://github.com/<owner>/<repo>` | 仓库根，默认 `ref=HEAD`，`path=.` |
| `https://github.com/<owner>/<repo>/tree/<ref>/<path>` | 指定分支/标签与子目录 |
| `https://github.com/<owner>/<repo>?ref=<ref>&path=<path>` | 查询参数形式 |
| `skills.sh:<source>/<skill>` | skills.sh ID |
| `https://skills.sh/<source>/<skill>` | skills.sh URL |
| `https://<其他 HTTPS URL>` | 通用 HTTPS 归档或目录 |
| 本地目录或归档路径 | 见《导入本地 Skill 目录》 |

GitHub URL 必须是无凭据的 HTTPS，拒绝 `user:password@` 和 `path=../` 穿越形式。

## 步骤

### 1. 搜索候选 Skill

```bash
rigdeck search skill "code review"
```

预期输出（TSV）：

```text
类型    名称    版本    Provider    定位符
Skill   review-checklist   1.2.0   skills.sh   skills.sh:acme/review-checklist
Skill   pr-lint            -       github      https://github.com/acme/skills/tree/main/pr-lint
```

### 2. 检查资产

挑一个定位符，先用 `add` 导入再 `inspect`。也可直接 `add` 远端来源（见下一步）。

### 3. 导入 Skill

```bash
rigdeck add "https://github.com/acme/skills/tree/main/pr-lint" --yes
```

预期输出：

```text
已导入 pr-lint（资产 asset_02...，修订 rev_02...）
```

### 4. 检查导入结果

```bash
rigdeck inspect asset_02...
```

预期输出：

```text
资产 ID：asset_02...
名称：pr-lint
类型：Skill
修订：rev_02...
来源：github
许可证：MIT
审计发现：N 项
```

阅读审计发现项；若包含 `hidden-executable` 或凭据采集类 high finding，停止安装并排查来源。

### 5. 分配到目标 Agent

```bash
rigdeck assign asset_02... --agent <instance-id> --scope project
```

记录返回的 `计划 ID`。

### 6. 应用计划

```bash
rigdeck apply <plan-id> --plan <plan-id> --yes
```

预期输出：

```text
计划 <plan-id> 已应用，完成 N 个操作
```

## 验证

```bash
rigdeck refresh --json
rigdeck doctor --json
```

`refresh` 退出码应为 `0`（`managed_clean`），`doctor` 全部检查项为 `通过`。目标 Agent 项目目录下出现 `pr-lint/SKILL.md`。
