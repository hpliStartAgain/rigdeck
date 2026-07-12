# 任务背景

RigDeck 要从当前仓库骨架推进为可交付的本地优先桌面产品：统一管理 Skill、Prompt/Rule 与 MCP Server，并通过运行时适配器支持 Claude Code、Codex、OpenCode、Hermes、Devin、Antigravity 和 Pi。

## 不可破坏的架构规则

1. Core 不出现 Agent 名称分支；Agent 差异全部位于运行时适配器。
2. 适配器不得直接写文件，只返回投影和规划操作。
3. 所有变更执行“计划 → 预览 → 备份 → 应用 → 验证 → 审计 → 可恢复”。
4. 桌面前端只通过 Tauri IPC 调用共享 Rust Core。
5. SQLite、日志、计划、备份和 JSON 输出只能出现 `SecretRef`，不得持久化明文 secret。
6. 新增文档、用户可见文本与关键 Rust 注释使用中文；Rust 注释重点解释所有权、借用、trait、错误传播与生命周期。

## 当前事实

- 分支：`codex/complete-rigdeck-ga`
- 初始提交：`fe8175a`
- 初始状态只有 workspace、CLI/Tauri 命令树、ADR 与主题令牌骨架，核心 crate 基本为空。
- Rustup `stable-msvc` 1.97.0、rustfmt、Clippy 已安装。
- 本机没有 Microsoft C++ Build Tools，原生链接暂由 CI 承担。
- 前端 `npm install` 已完成并生成锁文件。
- Notion 任务页：`39a2ced8-bde0-8186-87e5-d07abe0527e0`。

## 关键决策

- 维持 TODO 已确认的直接 GA 完整范围，不把缩减版本冒充 GA。
- 先完成可测试的共享 Core 与契约，再实现适配器和 UI，避免各入口复制业务逻辑。
- 外部签名证书、商标和域名属于发布前人工门禁，代码实现不能伪造这些条件已经满足。

## 当前最该读的文件

1. `TODO.md`：完整范围、阶段与验收门禁。
2. `AGENTS.md`：项目架构硬规则。
3. `.agent/STATE.json`：当前阶段、下一步与阻塞项。

## 2026-07-12 P0/P1 收口摘要

本轮完成 8 项 P0 和 5 项 P1 任务，全部 136 项测试通过：

- Adapter deprecation 字段、通用 URL provider、AssetRevision 扩展字段（author/update_time/platform_restrictions/vulnerabilities）、pin/archive/restore 生命周期、Prompt block 漂移检测、Case-only rename 检测、后台工作可取消。
- JSON Schema 文件（plan/conflict/audit_event）、GUI/CLI plan 等价性测试、故障注入与安全夹具、property-based 测试、CLI/Desktop 等价性测试。

关键修改文件：
- `crates/rigdeck-service/src/lib.rs`：取消机制、生命周期方法、等价性测试
- `crates/rigdeck-core/src/model.rs`：AssetRevision 扩展字段、property-based 测试
- `crates/rigdeck-core/src/refresh.rs`：CaseOnlyRename 检测
- `crates/rigdeck-adapters/src/bounded.rs`：classify_managed_document 重构
- `crates/rigdeck-registry/src/providers.rs`：UrlProvider
- `crates/rigdeck-registry/src/archive.rs`：故障注入测试
- `crates/rigdeck-security/src/redact.rs`：混沌测试
- `crates/rigdeck-core/schema/`：三个 JSON Schema 文件

