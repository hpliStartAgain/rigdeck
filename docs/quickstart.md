# 快速上手

5 分钟完成首次安装、检测、导入和分配。

## 安装与构建

前置条件：Rust 1.88+，Node.js 20 LTS。

```bash
# 克隆并构建 CLI
git clone <repo-url> rigdeck
cd rigdeck
cargo build --workspace

# 构建桌面端（可选）
cd apps/desktop
npm ci
npm run tauri dev
```

构建产物：

- Windows：`target\debug\rigdeck.exe`
- macOS：`target/release/rigdeck`

下文以 Windows 路径为例，macOS 替换为对应路径即可。

## 首次运行

```bash
# 查看版本与帮助
target\debug\rigdeck.exe --version
target\debug\rigdeck.exe --help
```

默认数据目录位于用户目录下。如需隔离测试或便携部署，用 `--data-dir` 覆盖：

```bash
target\debug\rigdeck.exe --data-dir D:\rigdeck-data status
```

## 检测 Agent

```bash
# 检测全部七种 Agent 的已安装实例
target\debug\rigdeck.exe agents detect

# 查看总览：Agent、资产、分配、冲突
target\debug\rigdeck.exe status
```

`agents detect` 输出每个 Agent 的实例路径、版本和管理目录。`status` 给出全局概况。

## 导入第一个 Skill

支持三种来源：本地目录/归档、`skills.sh:<id>`、GitHub URL。

```bash
# 从 skills.sh 导入
target\debug\rigdeck.exe add skills.sh:code-review --kind skill --yes

# 从本地目录导入
target\debug\rigdeck.exe add D:\skills\my-skill --kind skill --yes

# 从 GitHub 导入
target\debug\rigdeck.exe add https://github.com/user/skill-repo --kind skill --yes
```

导入后查看资产详情：

```bash
target\debug\rigdeck.exe inspect code-review
```

## 分配到 Agent

```bash
# 生成部署计划（不会修改 Agent 文件）
target\debug\rigdeck.exe assign code-review --agent claude-code --scope global
```

`--scope` 默认 `global`，可选项目级作用域。命令返回一个计划 ID。

## 应用计划

```bash
# 列出待应用计划
target\debug\rigdeck.exe plan

# 应用指定计划
target\debug\rigdeck.exe apply <plan-id> --yes
```

非交互场景必须传 `--yes`。机器调用还需传 `--plan <plan-id>` 防止参数错位。

应用后验证：

```bash
target\debug\rigdeck.exe status
```

## 下一步

- 完整功能参考 [用户指南](user-guide.md)
- 冲突处理见用户指南「冲突」章节
- 备份与恢复见用户指南「活动」章节
