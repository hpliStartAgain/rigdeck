# RigDeck 中英术语表 / Glossary

| 中文 | English | 精确定义 |
|---|---|---|
| 资产 | Asset | 可统一管理的 Skill、Prompt/Rule 或 MCP Server 逻辑实体 |
| 资产修订 | Asset Revision | 由 raw/normalized hash、来源、许可证与审计结果标识的不可变内容 |
| 来源 | Source | GitHub、skills.sh、MCP Registry、本地目录、URL、归档或私有提供者 |
| Agent 实例 | Agent Instance | 在一台机器或一个仓库中检测到的具体 Agent 安装/配置范围 |
| 适配器 | Adapter | 描述检测、扫描、编解码、投影与验证行为的版本化运行时扩展 |
| 分配 | Assignment | 将某个资产修订绑定到 Agent 实例和作用域的意图 |
| 投影 | Projection | 资产转换为某 Agent 原生格式后的目标表示；不负责直接写文件 |
| 部署计划 | Deployment Plan | 包含前置哈希、目标路径、diff、风险、兼容损失和回滚位置的文件级操作集合 |
| 部署快照 | Deployment Snapshot | 成功应用后用于漂移比较和恢复的不可变基线 |
| 漂移 | Drift | Agent 原生状态相对上次已知基线发生的外部变化 |
| 冲突 | Conflict | 无法安全自动合并、需要显式决策的语义差异 |
| 兼容损失 | Compatibility Loss | 源资产语义无法被目标 Agent 原生能力完整表达的部分 |
| 托管块 | Managed Block | RigDeck 在用户文档中拥有的带资产 ID/修订标记的有界文本区域 |
| secret 引用 | SecretRef | 指向系统钥匙串条目的不透明标识符，不包含明文凭据 |
| 手工处理 | manual_required | 官方接口不支持自动写入时生成的可验证导出和精确人工步骤 |
| 不支持 | unsupported | 目标 Agent 明确不具备该能力，必须包含原因和恢复/替代路径 |
| 内容寻址对象库 | Content-addressed Store | 按 Blake3 hash 存储不可变修订和备份、自动去重的本地对象库 |

## 用词规则

- UI、CLI 和文档统一使用“资产、修订、分配、计划、应用、漂移、冲突、恢复”。
- “覆盖”只能描述底层操作，不能包装成“冲突解决”。
- `Agent`、`Skill`、`Prompt`、`MCP Server` 作为产品领域名词保留英文，首次出现可附中文解释。

