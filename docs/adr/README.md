# 架构决策记录 / Architecture Decision Records

所有影响领域模型、持久化、公共协议、安全边界或跨平台行为的变更都必须更新现有 ADR 或新增 ADR。`Accepted` 表示决策已由项目目标确认；实现仍需通过各阶段验收门禁。

| 编号 | 决策 | 状态 | 日期 |
|---|---|---|---|
| 0001 | MIT 许可证 | Accepted | 2026-07-11 |
| 0002 | Tauri + React + Rust 技术栈 | Accepted | 2026-07-11 |
| 0003 | 统一领域模型与资产身份 | Accepted | 2026-07-11 |
| 0004 | 运行时 Adapter Contract | Accepted | 2026-07-11 |
| 0005 | SQLite + 内容寻址对象库 + 系统钥匙串 | Accepted | 2026-07-11 |
| 0006 | 可恢复事务引擎 | Accepted | 2026-07-11 |
| 0007 | 三方冲突处理 | Accepted | 2026-07-11 |
| 0008 | STRIDE 与失败关闭安全模型 | Accepted | 2026-07-11 |
| 0009 | 品牌和设计令牌主题 | Accepted | 2026-07-11 |

## 状态规则

- `Proposed`：需要确认，不能作为稳定公共契约。
- `Accepted`：决策有效，实现与测试必须遵守。
- `Superseded`：被后续 ADR 替代，保留历史原因和替代编号。
- `Rejected`：记录已评估但不采用的方案，避免重复踩坑。

