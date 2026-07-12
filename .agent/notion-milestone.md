---
今日重点: false
任务层级: 项目级
任务类型: 代码开发
优先级: P1 重要
停滞天数: 0
关联OKR: []
原始文件链接: null
启动日期: null
周期: 2026-H2
子任务列表: []
完成日期: null
工作域: 未分类
已完成?: false
截止日期: null
标签: []
标题: 完整交付 RigDeck 多 Agent 能力管理器
父任务: []
状态: 进行中
超期预警: null
项目阶段: M2 Adapter 与本地刷新
---

# 完整交付 RigDeck 多 Agent 能力管理器

## 2026-07-11 启动与基线

- 分支：`codex/complete-rigdeck-ga`
- 已建立 `.agent/CONTEXT.md`、`.agent/PROGRESS.md`、`.agent/STATE.json`。
- 本机 Windows 缺少 Microsoft C++ Build Tools，正式原生工件由 Windows/macOS CI 与后续干净机矩阵验证。

## M0 已完成：产品契约与品牌

- 中文/英文 PRD、术语表、STRIDE 威胁模型、贡献指南、发布政策、迁移规范和 ADR。
- 完整 SVG/PNG/ICO/ICNS/favicon/CLI/social-preview 品牌资产与对比度验收。
- Rust 1.88 MSRV、Cargo/npm 锁文件和跨平台 CI 基线。

## M1 已完成：Core、Store、Planner 与 Adapter SDK

- 统一资产、修订、来源、实例、分配、投影、计划、快照、冲突、SecretRef 和审计模型。
- Adapter Protocol v1、JSON Schema、JSON-RPC helper 合同、信任 hash 与路径安全测试。
- SQLite v2、加密内容寻址对象库、系统钥匙串抽象、migration 保护恢复与完整性检查。
- 文件级 Planner、原子写入、每步故障注入、文件/数据库共同回滚、幂等和审计。

## M2 进行中：七个 Adapter 与本地刷新

- 已完成 Claude Code、Codex、OpenCode、Hermes、Antigravity、Pi、Devin 七份内置 manifest 和官方来源能力矩阵。
- 已完成托管块、JSONC/TOML/YAML 保真补丁、MCP 原生编码、SecretRef 阻断占位、投影物化和精确卸载意图。
- 已完成每次启动检测/扫描、漂移和 rename 分类、可解释冲突、watcher 去抖、SQLite 库存与冲突动作审计。
- 当前验证：Core 12 项、Adapter 23 项、Store 11 项测试通过；相关 crate 严格 Clippy 通过。

## 下一步

完成 Adapter scaffold/validate/test/pack、Windows/macOS 黄金夹具和完整 Skill bundle 生命周期；随后进入 CLI、桌面端双语主题与 `rigdeck-manager` 端到端实现。

## 外部门禁

- Windows 代码签名证书尚未购买。
- Apple Developer Program 尚未开通。
- 商标、域名与签名属于发布前人工门禁，不会伪造完成状态。
