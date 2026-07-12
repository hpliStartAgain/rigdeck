# 用户指南

RigDeck 桌面端与 CLI 的完整使用说明。

桌面端有 7 个主页面：总览、资产库、Agent、装配、冲突、活动、设置。左侧主导航切换，支持键盘操作。

首次安装与基础流程见 [快速上手](quickstart.md)，本指南不重复。

## 总览

Agent 和资产概况，启动时自动扫描。

- 顶部展示 Agent 数量、资产数量、已启用分配数、未解决冲突数
- Agent 状态区列出每个 Agent 的健康状态
- 提醒区显示待处理事项：可用更新、高风险计划待确认
- 点击「检查更新」刷新来源版本
- 点击「刷新」重新扫描 Agent 原生状态

## 资产库

管理 Skill、Prompt 和 MCP Server，分「已导入」和「远程目录」两个视图。

### 导入资产

1. 在导入框输入来源：本地路径、`skills.sh:<id>` 或 GitHub URL
2. 选择类型：Skill / Prompt / MCP
3. 点击「导入」

Prompt 从本地 `.md` / `.txt` 文件导入。MCP 从规范化 JSON 文件导入。

### 搜索与筛选

- 顶部搜索框按名称或来源过滤本地资产
- 来源下拉筛选 Skill / Prompt / MCP
- 远程目录视图搜索 skills.sh 和 MCP Registry

### 查看资产详情

点击资产卡片查看：来源、许可证、审计风险、文件列表、兼容 Agent。

### 资产生命周期操作

| 操作 | 说明 |
|------|------|
| 固定 | 锁定当前修订，`update` 不再生成更新计划 |
| 解除固定 | 恢复更新检查 |
| 归档 | 保留修订和备份，从活跃列表移除 |
| 恢复资产 | 从归档恢复到活跃列表 |
| 卸载 | 生成精确卸载计划 |

## Agent

已安装 Agent 的实例、版本、管理目录和能力。

- 每个 Agent 实例显示 Profile、版本、管理目录路径
- 能力区显示该 Agent 支持的作用域和写入模式
- 已分配资产区列出该 Agent 的全部分配及启用状态
- 点击「启用」或「停用」生成对应计划，跳转装配页确认

## 装配

把资产分配到 Agent，生成计划并应用。

### 生成计划

1. 选择资产
2. 选择目标 Agent
3. 选择作用域（global / 项目级）
4. 点击「生成计划」

### 预览计划

计划预览区显示：

- 计划 ID、类型、风险等级
- 每个操作的目标路径、操作类型、文件 diff
- 回滚可用性
- 兼容性损失提示

### 确认应用

1. 输入计划 ID 末 8 位确认码
2. 点击「应用」
3. 应用完成后自动刷新验证

高风险计划会额外标注，确认前务必检查 diff。

## 冲突

处理 Agent 配置与 RigDeck 基线的三方冲突。

### 冲突来源

- 托管内容被 Agent 外部修改
- 双边同时修改
- 卸载后被重新创建
- MCP Key 冲突
- 托管块损坏

### 解决冲突

1. 在冲突列表点击条目查看详情
2. 查看三方对比：基线、RigDeck 版本、Agent 当前版本
3. 选择解决操作：
   - 用 RigDeck 版本
   - 保留 Agent 版本
   - 用合并结果（可粘贴手动合并内容）
   - 重命名并共存
4. 点击「生成解决计划」
5. 跳转装配页确认应用

## 活动

操作记录和备份管理。

### 操作记录

时间线列出全部写操作：计划应用、备份恢复、刷新、冲突解决。点击条目查看详情。

### 备份

- 点击「创建备份」生成 SQLite 与对象库的一致性备份
- 备份列表显示备份 ID、对象数量、时间
- 点击「恢复」从备份恢复，恢复前自动创建 recovery 备份

## 设置

语言、主题、诊断和安全。

- 语言：简体中文 / English
- 主题：跟随系统 / Porcelain / Obsidian / Aurora
- 诊断：运行 `doctor` 检查数据库、对象库、Adapter 和路径
- 安全：凭据存储在系统钥匙串，不出现在数据库、日志或导出包中

## CLI 基本用法

CLI 与桌面端共享同一 Rust Core，产生字节等价的计划。所有命令支持 `--json` 输出稳定、带版本的 JSON，供自动化调用。

### 全局选项

```bash
rigdeck --json                     # JSON 输出
rigdeck --data-dir <path>          # 覆盖数据目录
rigdeck --project-root <path>      # 指定项目根目录
```

### 搜索资产

```bash
# 搜索 skills.sh
rigdeck search skill <query>

# 搜索 MCP Registry
rigdeck search mcp <query>
```

### 查看资产详情

```bash
rigdeck inspect <asset>
```

### 管理分配状态

```bash
# 列出全部分配
rigdeck assignments list

# 生成启用计划
rigdeck assignments enable <assignment-id>

# 生成停用计划
rigdeck assignments disable <assignment-id>
```

### 检查更新

```bash
# 刷新来源并为可更新资产生成计划
rigdeck update --yes
```

### 卸载资产

```bash
rigdeck remove <asset> --yes
```

### 固定与解除固定

```bash
rigdeck pin <asset>
rigdeck unpin <asset>
```

### 归档与恢复

```bash
rigdeck archive <asset>
rigdeck restore-asset <asset>
```

### 冲突管理

```bash
# 列出未解决冲突
rigdeck conflicts list

# 查看冲突详情
rigdeck conflicts show <conflict-id>

# 记录解决动作
rigdeck conflicts resolve <conflict-id> --action <action>
```

### 备份与恢复

```bash
# 创建一致性备份
rigdeck backup

# 从备份恢复
rigdeck restore <backup-id> --yes
```

### 诊断

```bash
rigdeck doctor
```

检查数据库完整性、对象库一致性、Adapter 契约和路径权限。

### 导入导出

```bash
# 导出不含明文 secret 的配置包
rigdeck export <output-path>

# 导入配置包
rigdeck import <input-path> --yes
```

### Secret 管理

Secret 值从不通过命令行参数传递，从标准输入读取。

```bash
# 写入 secret（从 stdin）
echo "my-token" | rigdeck secret set <secret-ref>

# 检查是否存在
rigdeck secret check <secret-ref>

# 删除（幂等）
rigdeck secret delete <secret-ref>
```

### Adapter 开发

```bash
# 列出内置 Adapter
rigdeck adapter list

# 验证 Adapter 包
rigdeck adapter validate <path>

# 运行契约测试
rigdeck adapter test <path>

# 创建脚手架
rigdeck adapter scaffold <name>

# 创建分发 bundle
rigdeck adapter pack <path>
```

### 计划管理

```bash
# 列出待应用计划
rigdeck plan

# 应用计划
rigdeck apply <plan-id> --yes
```

机器调用必须额外传 `--plan <plan-id>`，防止参数拼接错位。
