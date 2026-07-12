# 核心概念

RigDeck 的领域模型由以下概念组成。每个概念给出定义和关键属性，细节实现见 `docs/adr/`。

## 资产（Asset）

可被统一管理的逻辑实体，分三种类型。

- 类型：`skill` | `prompt` | `mcp_server`，对应 Skill 目录、Prompt/Rule 片段、MCP Server 定义。
- 身份：`source_namespace + package + relative_path + declared_name` 四元组，生成稳定 ID。不以目录名作为唯一标识，避免同名冲突和来源混淆。
- 生命周期：来源采集 → 修订 → 分配 → 计划 → 应用 → 漂移/冲突 → 备份/恢复，三类资产共用同一套。

## 修订（Revision）

资产内容的不可变快照，进入内容寻址对象库后不再变更。

- 哈希：`normalized_hash`（Blake3，规范化后）与 `raw_hash`（原始字节）双标识。
- 来源：记录 Source 类型、命名空间、许可证和静态审计结果。
- 不可变：同一内容只存一份，自动去重；任何修改产生新修订而非就地覆盖。

## 分配（Assignment）

将某个资产修订绑定到 Agent 实例和作用域的意图记录。

- 绑定对象：资产修订 + Agent 实例 + 作用域（个人/项目/全局等）。
- 性质：是意图而非已生效状态；是否真正落地以 DeploymentSnapshot 为准。
- 重复应用幂等：对已成功的分配再次生成计划，结果为空操作。

## 计划（Plan）

文件级操作清单，任何写盘动作必须先有计划。

- 内容：前置条件、输入哈希（源 + 目标）、目标路径、渲染 diff、兼容损失、风险等级、回滚位置。
- 强制流程：生成 → 预览 → 确认 → 备份 → 原子应用 → 哈希验证 → 审计，缺一不可。
- 失效条件：应用前若源或目标哈希变化，计划自动作废，必须重新生成。
- GUI 与 CLI 对相同输入生成字节等价的计划。

## 冲突（Conflict）

RigDeck 修订与 Agent 当前状态之间无法安全自动合并的差异，必须显式决策。

- 三方输入：Base（上次成功应用的快照）、Ours（RigDeck 修订重渲染）、Theirs（Agent 侧当前文件）。
- 分类：NameCollision、ManagedModified、ConcurrentModification、DamagedManagedBlock、McpKeyCollision 等 14 种。
- 解决动作：采用 RigDeck 修订、导入 Agent 修订、保留分叉、重命名共存、三方合并、逐文件选择、放弃计划、从备份恢复，共 8 种。
- 规则：强制覆盖不等于冲突解决；只有关联计划成功提交后冲突才标记已解决，"放弃计划"例外。

## 漂移（Drift）

Agent 端文件被 RigDeck 之外的力量修改，相对上次基线产生偏离。

- 检测时机：桌面启动全量扫描；运行中监听 Agent 原生配置变化。
- 状态分类：`managed_clean`、`managed_modified`、`external_new`、`external_removed`、`source_update_available`、`conflict`、`unsupported`、`manual_required`。
- 区别于冲突：漂移是客观状态，冲突是需要决策的语义差异。双边修改的漂移升级为冲突。

## Adapter

运行时插件，声明某个 Agent 的检测路径、原生格式和能力边界。

- 契约：`adapter.json` 声明 ID、版本、平台、检测规则、能力矩阵、原生编解码、限制。
- 方法：`describe`、`detect`、`scan`、`validate_asset`、`render`、`plan_install`、`plan_update`、`plan_remove`、`verify`、`health`。
- 硬规则：Adapter 只返回 Projection 和操作意图，不直接写文件；Core 不含 Agent 名称分支。
- 安装方式：内置适配器随版本发布；第三方适配器打包后通过 `rigdeck adapter` 命令加载，需显式信任。

## SecretRef

指向系统钥匙串条目的不透明标识符，用于引用 MCP 凭据等敏感数据。

- 存储：只存标识符到 SQLite，明文只存在于 OS 钥匙串（Windows Credential Manager / macOS Keychain）。
- 脱敏范围：日志、诊断、崩溃输出、计划、UI 复制、JSON CLI 输出、导出包一律不出现明文。
- 物化时机：仅当 Agent 无法间接引用 secret 且用户通过高风险确认后，才取出明文。
