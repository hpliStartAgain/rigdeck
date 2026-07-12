# 变更日志

RigDeck 的用户可感知变更记录在此。版本遵循 SemVer；发布分级与门禁见 `docs/release-policy.md`。

## [Unreleased]

### 新增 — 2026-07-12

- Adapter manifest 支持 `deprecation` 字段（`superseded_by`、`sunset_at`、`reason`），`validate()` 检查日期格式与循环引用，schema 与单元测试同步更新。
- 通用 URL provider：`UrlProvider` 处理指向 archive 或单 `SKILL.md` 的 HTTPS 链接，集成到 `add_remote_skill` 和 `update_assets`。
- `AssetRevision` 扩展 `author`、`update_time_ms`、`platform_restrictions` 字段，TypeScript 类型同步更新。
- `AssetRevision` 扩展 `vulnerabilities` 和 `vulnerability_score` 字段，TypeScript 类型同步更新。
- pin/unpin/archive/restore 资产生命周期方法落地，含状态转换、审计事件和单元测试。
- Prompt block 漂移与结构损坏检测：`classify_managed_document` 返回 `ManagedBlockIssue` 枚举，`builtin` 据此报告 `PromptBlockMoved` 或 `DamagedManagedBlock`。
- Case-only rename 冲突检测：`classify_refresh` 识别仅大小写不同的路径重命名并标记为 `ConflictKind::CaseOnlyRename`。
- 后台工作可取消：`RigDeckService` 新增 `request_cancel`/`clear_cancel`/`is_cancelled`，`update_assets` 在每个资产边界检查取消标志并提前返回；`ServiceError::Cancelled` 变体落地。
- JSON Schema 文件：`plan.schema.json`、`conflict.schema.json`、`audit_event.schema.json` 描述 CLI/Tauri IPC 共用的 JSON 格式。
- GUI/CLI plan 等价性测试：验证相同输入产生确定性等价的 operations。
- 故障注入与安全夹具：损坏归档、空目录、tar 路径逃逸、redact 边界输入测试。
- Property-based 测试：ContentHash 确定性与区分性、AssetIdentity 稳定 ID、SecretRef 模式拒绝。
- CLI/Desktop 等价性测试：验证 CLI 和 Tauri IPC 共用的核心 service 方法签名一致。

### 新增 — 2026-07-11

- 初始化包含 7 个 Rust crate、Tauri/React 桌面端和独立 CLI 的 Cargo workspace。
- 建立完整 GA 范围的 9 阶段交付清单与 9 份架构决策记录。
- 提供中文为主、英文摘要为辅的 PRD、术语表、STRIDE 威胁模型、贡献指南、发布政策和旧 Registry 一次性迁移规范。
- 完成 RigDeck 几何品牌：master/单色/横向 SVG、16–1024 PNG、favicon、CLI mark、Windows ICO、macOS ICNS 和 GitHub social preview。
- 增加可复现品牌生成脚本和锁定的 `@resvg/resvg-js` 构建依赖。
- 增加 Windows/macOS Rust CI 与 Windows 前端 CI。
- 提供 `rigdeck-manager` Skill 的安全操作工作流骨架。

### 变更 — 2026-07-11

- 根据锁定依赖元数据将工作区 MSRV 从未经验证的 1.75 调整为 1.88。
- 将 `Cargo.lock` 与 `package-lock.json` 纳入版本控制，保证应用/CLI 构建可复现。
- 架构 ADR 0003/0004/0006/0008 由 Proposed 更新为 Accepted。
- 新增 `.agent/` 三件套与 Notion 项目任务，支持跨会话持续交付。

### 已知门禁

- 本机尚未安装 Microsoft C++ Build Tools，Windows 原生链接由 CI/后续本机环境验收。
- Windows 代码签名证书、Apple Developer Program、正式商标检索和域名预留仍需发布前人工完成。

