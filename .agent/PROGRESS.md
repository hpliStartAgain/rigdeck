# 执行进展

## 2026-07-11 — 基线与环境

- 接续 Codex 持续目标并判定为 COMPLEX。
- 创建分支 `codex/complete-rigdeck-ga`，未提交、未推送。
- 创建 Notion 项目级任务“完整交付 RigDeck 多 Agent 能力管理器”。
- 完整读取项目规范、`task-complex`、`notion-task-sync`、Notion 与自学习 Skill。
- 盘点仓库：13 项已勾选、265 项待办，Rust 业务 crate 仍为空骨架。
- 安装 Rustup `stable-msvc` 1.97.0、rustfmt 与 Clippy；修复一次并发 Rustup 子进程造成的半安装状态。
- 完成前端依赖安装并生成 `apps/desktop/package-lock.json`。
- 运行 `cargo fmt` 修复初始 Rust 格式差异。

## 2026-07-11 — M0 产品契约与品牌

- 完成中文/英文 PRD、术语表、STRIDE 威胁模型、贡献指南、发布政策与旧 Registry 一次性迁移规范。
- 将实际依赖图 MSRV 固定为 Rust 1.88，生成并纳入 Cargo/npm 锁文件。
- 完成 D 形甲板 + 三模块几何品牌及 SVG、PNG、ICO、ICNS、favicon、CLI、社交预览全套资产。
- 视觉检查 16px/1024px/social preview；对比度：主色/白 8.72:1，accent/深色 9.59:1。
- 建立 Windows/macOS Rust CI 与 Windows 前端 CI。
- 验证通过：`cargo fmt --all -- --check`、`npm run build`。

## 2026-07-11 — M1 Core、Store 与 Adapter SDK

- 完成统一 `Asset`/Revision/Source/Assignment/Projection/Plan/Snapshot/Conflict/SecretRef/AuditEvent 模型及 Blake3 双 hash。
- 完成 Adapter Protocol v1、`adapter.json` JSON Schema、声明式检测、JSON-RPC 消息、helper hash 信任和路径逃逸测试。
- 完成 SQLite v2 migration、加密内容寻址对象库、系统钥匙串抽象、migration 保护恢复与完整性检查。
- 完成文件级 Planner、原子替换、逐步骤故障注入、数据库提交回滚边界、幂等与审计。
- Core/SDK/Store/Security 共 30 余项单元和集成测试通过；相关 crate 严格 Clippy 通过。

## 2026-07-11 — M2 Adapter 与本地刷新（进行中）

- 建立七个内置 Adapter manifest 与 2026-07-11 官方来源能力矩阵。
- 将整文件、Skill 目录、托管块和结构化条目提升为 Agent 无关的投影策略；Core 仍无 Agent 名称分支。
- 完成边界块编解码，保留 BOM、CRLF 和块外字节，损坏/重复/嵌套块失败关闭。
- 完成 JSONC、TOML、YAML 定点 MCP 条目补丁，保留注释、未知字段和无关排版。
- 完成 Claude/Codex/OpenCode/Hermes/Antigravity MCP 原生编码；SecretRef 只产生阻断占位，不写明文。
- 完成投影物化到 Planner、精确卸载意图、实例根目录越界拒绝。
- 完成启动检测/扫描、rename 和漂移/冲突分类、watcher 去抖、SQLite 库存刷新与冲突解决审计。
- Adapter 23 项测试、Core 12 项测试、Store 11 项测试及严格 Clippy 通过。

## 2026-07-12 — P0/P1 功能与测试收口

- P0-1：Adapter manifest `deprecation` 字段（superseded_by/sunset_at/reason），validate 检查循环引用与日期格式，schema 与测试同步。
- P0-2：通用 URL provider 落地，处理 HTTPS archive 和单 SKILL.md，集成到 add_remote_skill/update_assets。
- P0-3：AssetRevision 扩展 author/update_time_ms/platform_restrictions，所有构造点和 TS 类型同步。
- P0-4：pin/unpin/archive/restore 生命周期方法落地，含状态转换、审计事件和单元测试。
- P0-5：AssetRevision 扩展 vulnerabilities/vulnerability_score，importer 和 TS 类型同步。
- P0-6：Prompt block 漂移与结构损坏检测，classify_managed_document 返回 ManagedBlockIssue 枚举。
- P0-7：Case-only rename 冲突检测，classify_refresh 标记 CaseOnlyRename。
- P0-8：后台工作可取消，request_cancel/clear_cancel/is_cancelled，update_assets 协作式取消，ServiceError::Cancelled。
- P1-1：GUI/CLI plan 等价性测试，验证 operations 确定性等价。
- P1-2：JSON Schema 文件 plan/conflict/audit_event.schema.json，含合法性测试。
- P1-3：故障注入与安全夹具，损坏归档/空目录/tar 路径逃逸/redact 边界输入。
- P1-4：Property-based 测试，ContentHash/AssetIdentity/SecretRef 属性验证。
- P1-5：CLI/Desktop 等价性测试，验证核心 service 方法签名一致。
- 全 workspace 8 crate 共 136 项测试通过。
