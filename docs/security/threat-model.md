# RigDeck STRIDE 威胁模型 / Threat Model

- 状态：已确认，随安全相关变更更新
- 范围：来源、归档、适配器/helper、MCP 凭据、更新器、SQLite/对象库、文件系统写入、CLI/Tauri IPC

**English summary:** RigDeck treats remote assets, archives, adapter helpers, and Agent-native configuration as untrusted inputs. Every mutation is path-scoped, planned, backed up, verified, audited, and recoverable.

## 信任边界

1. 远端来源 → Registry Provider：网络响应与仓库内容不受信任。
2. 归档/本地目录 → 安全检查器：文件名、链接、大小和内容不受信任。
3. 第三方 Adapter/helper → Core：声明元数据可读，执行 helper 前必须显式信任。
4. Frontend/CLI → Core：参数不因来自本机 UI 而自动可信。
5. Core → Agent 文件系统：只能写批准计划列出的规范化目标。
6. Core → 系统钥匙串：数据库只保存 `SecretRef`；明文仅在授权边界内短暂存在。

## STRIDE 清单

| 类型 | 主要威胁 | 强制缓解 | 验证证据 |
|---|---|---|---|
| Spoofing | 伪造来源命名空间、适配器 ID、MCP 包 | 来源 provenance、namespace 验证、签名/校验和、adapter ID 唯一性 | provenance 与清单伪造夹具 |
| Tampering | 计划生成后源/目标被改；备份或更新清单被替换 | 应用前重算 hash；对象库按 hash 校验；签名更新清单；SQLite 事务 | stale-plan、对象损坏、签名失败测试 |
| Repudiation | 无法证明谁在何时应用或解决冲突 | 追加式 `AuditEvent`，记录计划/快照/决策 ID，不记录 secret | 审计事件集成测试 |
| Information Disclosure | secret 出现在 DB、日志、JSON、备份、剪贴板 | `SecretRef`、系统钥匙串、结构化脱敏、禁止 provider URL 携带凭据 | secret 扫描夹具 |
| Denial of Service | archive bomb、巨量文件、watcher 风暴、10k 资产冻结 UI | 文件数/单文件/总大小/深度上限；有界队列、去抖、取消令牌、后台分页 | 归档与性能测试 |
| Elevation of Privilege | 路径穿越、symlink escape、helper 执行任意命令、Tauri 权限过宽 | canonical path + 根目录约束；拒绝链接逃逸；helper 信任与沙箱；最小 IPC capability | traversal/symlink/helper/IPC 测试 |

## 默认限制

- 被动浏览不会运行 Skill 脚本或 MCP health probe。
- 可执行 helper 默认禁用；信任决定绑定 adapter ID + hash，更新后重新确认。
- 归档拒绝绝对路径、`..` 逃逸、设备路径、NTFS ADS、循环链接和超限内容。
- Agent 不支持 secret 间接引用时，只有高风险确认后才可向计划目标短暂物化明文。
- crash/diagnostic/export 默认脱敏；复制 UI 文本使用同一脱敏器。

## 剩余风险

- 静态审计只能提示可疑内容，不能证明 Skill 安全。
- 已被用户信任的 helper 仍可能恶意；应尽可能限制工作目录、环境变量、网络和文件范围。
- 本机管理员或已入侵进程可读取用户态数据；RigDeck 不宣称抵御已完全失陷的主机。

## 安全事件响应

发现泄漏或越界写入时：停止自动应用、保留脱敏审计证据、将相关计划标为不可用、提供确定性恢复步骤，并在修复与回归夹具完成前阻止发布。

