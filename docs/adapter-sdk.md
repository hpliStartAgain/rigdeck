# Adapter SDK 开发指南

为新的 AI Agent 编写适配器，接入 RigDeck 的检测、扫描、编解码、投影与验证流水线。

## Adapter 是什么

Adapter 是一个版本化的运行时扩展，描述某个 AI Agent 的检测规则、资产能力矩阵、原生格式编解码和投影方式。它只返回投影（Projection）和操作意图（AdapterPlan），从不直接写 Agent 文件；所有写操作由 Core Planner 在计划 → 预览 → 备份 → 应用 → 验证 → 审计的流水线内完成。新增 Agent 无需重新编译 Core。

## Adapter manifest 字段

`adapter.json` 是每个 Adapter 包的声明式清单，遵循 [adapter-v1 JSON Schema](../crates/rigdeck-adapter-sdk/schema/adapter.schema.json)。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `schema_version` | `1` | 是 | Manifest schema 版本，当前固定为 `1` |
| `adapter_id` | string | 是 | 小写稳定 ID，仅 `a-z0-9._-`，如 `claude-code` |
| `version` | string | 是 | Adapter 包语义版本，如 `1.0.0` |
| `display_name` | string | 是 | 用户可读名称，如 `Claude Code` |
| `protocol` | object | 是 | 兼容的 RigDeck 协议范围 `{ "min": 1, "max": 1 }`，须包含当前协议版本 `1` |
| `platforms` | array | 是 | 支持平台，枚举 `windows` / `macos` / `linux`，至少一个 |
| `detection` | array | 是 | 检测规则，见下表 |
| `capabilities` | array | 是 | 资产能力矩阵，见下表 |
| `scopes` | array | 是 | 作用域描述，见下表 |
| `native_formats` | array | 是 | 原生格式及保真声明，见下表 |
| `codecs` | array | 是 | 输入资产到原生格式的编解码器，见下表 |
| `surfaces` | array | 是 | 资产作用域到本地路径的映射，见下表 |
| `limitations` | array | 否 | 已知限制，每项含 `code` / `message` / `recovery` |
| `official_docs` | array | 是 | 官方文档 URL |
| `helper` | object\|null | 否 | JSON-RPC helper，见下表 |
| `deprecation` | object\|null | 否 | 弃用声明，见下表 |

### detection 规则

| 字段 | 类型 | 说明 |
|---|---|---|
| `path` | string | 路径模板，必须以 `{home}` 或 `{project}` 开头，禁止 `..` 和绝对路径 |
| `markers` | array | 路径存在后还须存在的相对 marker 文件 |
| `profile` | string\|null | 多实例 profile 名，空表示默认实例 |
| `version_hint` | string\|null | 可选版本提取提示；声明式检测器不会执行命令 |

### capabilities 能力矩阵

| 字段 | 类型 | 说明 |
|---|---|---|
| `asset_kind` | enum | `skill` / `prompt` / `mcp_server` |
| `scopes` | array | 该能力适用的 scope ID，须已在 `scopes` 中声明 |
| `operations` | array | 支持的操作，枚举 `import` / `install` / `update` / `toggle` / `remove` / `drift` |

### scopes 作用域

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 稳定 scope ID，如 `global` / `project` / `repository` |
| `display_name` | string | 用户可读名称 |
| `project_required` | bool | 是否需要 project root |

### native_formats 原生格式

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 格式 ID，如 `jsonc` / `toml` / `yaml` / `markdown` / `skill-directory` |
| `preserves_comments` | bool | 是否保留注释 |
| `preserves_unknown_fields` | bool | 是否保留未知字段 |

### codecs 编解码器

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | Codec ID，如 `agent-skill-v1` |
| `asset_kind` | enum | 输入资产种类 |
| `native_format` | string | 输出原生格式 ID，须已在 `native_formats` 中声明 |

### surfaces 投影表面

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | Adapter 内稳定 surface ID，不可重复 |
| `scope` | string | 对应已声明 scope |
| `asset_kind` | enum | 接受的资产种类 |
| `root` | string | 允许根路径，以 `{home}` 或 `{project}` 开头 |
| `target` | string | 相对 root 的目标模板，只允许可选的 `{name}` |
| `native_format` | string | 原生格式 ID |
| `section` | string\|null | 结构化配置顶层 section；仅 `structured_entry` 模式可填 |
| `mode` | enum | 投影方式，见下节 |
| `writable` | bool | 是否允许创建新投影；旧兼容路径应为 `false` |
| `precedence` | integer | 冲突优先级，数字越小越优先，范围 `0..=65535` |

### helper（可选）

| 字段 | 类型 | 说明 |
|---|---|---|
| `executable` | string | 包内相对可执行路径，禁止 `..` 和绝对路径 |
| `hash` | string | helper 文件 Blake3 hash，64 位十六进制 |
| `protocol` | const | 必须为 `jsonrpc-2.0` |
| `requested_access` | array | 向用户披露的请求权限说明 |

加载 manifest 不会执行 helper；运行前须取得绑定 `adapter_id + hash` 的显式信任决定。

### deprecation（可选）

| 字段 | 类型 | 说明 |
|---|---|---|
| `sunset_version` | string | 计划移除的 Adapter 包版本 |
| `replacement` | string\|null | 替代 adapter ID，不能指向自身 |
| `notice` | string | 弃用说明或迁移指引 |

存在时，Core 在加载和计划阶段向用户发出可见警告。

## 开发流程

整体顺序：scaffold → 编辑 manifest → 实现 trait → validate → test → pack。

### 1. scaffold 生成骨架

```bash
rigdeck adapter scaffold --name my-agent --kind my-agent
```

生成的目录结构：

```
my-agent/
├── adapter.json          # 可立即验证的最小 manifest
├── README.md
└── fixtures/
    ├── windows/
    │   ├── home/.my-agent/installed.marker
    │   └── project/
    └── macos/
        ├── home/.my-agent/installed.marker
        └── project/
```

骨架 manifest 已包含一组默认声明：`skill` 资产、`global` 作用域、`skill-directory` 原生格式、`directory_tree` 投影表面和 `windows`/`macos`/`linux` 平台。

### 2. 编辑 manifest

按目标 Agent 的实际能力修改 `adapter.json`。参考内置适配器示例：

```json
{
  "schema_version": 1,
  "adapter_id": "my-agent",
  "version": "0.1.0",
  "display_name": "My Agent",
  "protocol": { "min": 1, "max": 1 },
  "platforms": ["windows", "macos"],
  "detection": [
    { "path": "{home}/.my-agent", "markers": ["installed.marker"], "profile": null, "version_hint": null },
    { "path": "{project}/.my-agent", "markers": [], "profile": "project", "version_hint": null }
  ],
  "capabilities": [
    { "asset_kind": "skill", "scopes": ["global", "project"], "operations": ["import", "install", "update", "remove", "drift"] },
    { "asset_kind": "prompt", "scopes": ["global"], "operations": ["install", "update", "remove"] }
  ],
  "scopes": [
    { "id": "global", "display_name": "全局", "project_required": false },
    { "id": "project", "display_name": "项目", "project_required": true }
  ],
  "native_formats": [
    { "id": "skill-directory", "preserves_comments": true, "preserves_unknown_fields": true },
    { "id": "markdown", "preserves_comments": true, "preserves_unknown_fields": true }
  ],
  "codecs": [
    { "id": "agent-skill-v1", "asset_kind": "skill", "native_format": "skill-directory" },
    { "id": "bounded-markdown-v1", "asset_kind": "prompt", "native_format": "markdown" }
  ],
  "surfaces": [
    { "id": "global-skills", "scope": "global", "asset_kind": "skill", "root": "{home}", "target": ".my-agent/skills/{name}", "native_format": "skill-directory", "section": null, "mode": "directory_tree", "writable": true, "precedence": 0 },
    { "id": "global-prompt", "scope": "global", "asset_kind": "prompt", "root": "{home}", "target": ".my-agent/AGENTS.md", "native_format": "markdown", "section": null, "mode": "managed_block", "writable": true, "precedence": 0 }
  ],
  "official_docs": ["https://example.invalid/replace-with-official-docs"],
  "helper": null
}
```

跨字段语义校验规则（manifest `validate()` 强制）：

- `protocol.min..=max` 必须包含当前协议版本 `1`。
- `capabilities` 和 `surfaces` 引用的 scope 必须在 `scopes` 中声明。
- `surfaces` 引用的 `native_format` 必须在 `native_formats` 中声明。
- 每个 surface 必须有对应的 capability（同 `asset_kind` + `scope`）。
- `structured_entry` 模式必须声明 `section`；其他模式不能声明 `section`。
- `target` 模板只能使用 `{name}` 变量，不允许其他 `{...}`。

### 3. 实现 trait

实现 `rigdeck_adapter_sdk::AgentAdapter` trait。所有方法接收 `&self`，trait 要求 `Send + Sync`。

| 方法 | 输入 | 输出 | 职责 |
|---|---|---|---|
| `describe` | `&self` | `&AdapterManifest` | 返回 manifest 引用 |
| `detect` | `&DetectionContext` | `Vec<AgentInstance>` | 按 detection 规则发现实例 |
| `scan` | `&AgentInstance` | `Vec<ScannedEntry>` | 扫描原生状态，返回路径、hash、managed 标记 |
| `validate_asset` | `&Asset`, `&AssetRevision`, `&AgentInstance`, `scope` | `AssetValidation` | 验证资产能否投影，返回 warnings/errors |
| `render` | `&Asset`, `&AssetRevision`, `&[u8]`, `&AgentInstance`, `scope`, `&[SecretRef]` | `RenderOutput` | 渲染为原生格式，内容写入对象库而非目标文件 |
| `render_bundle` | 同上但传 `&AssetContent` | `RenderOutput` | 渲染多文件 bundle，默认实现委托单文件 `render` |
| `plan_install` | `&Projection` | `AdapterPlan` | 规划安装操作意图 |
| `plan_update` | `&Projection` | `AdapterPlan` | 规划更新操作意图 |
| `plan_remove` | `&Asset`, `&AgentInstance`, `scope` | `AdapterPlan` | 规划精确卸载，不删除共享文件 |
| `verify` | `&Projection` | `VerificationReport` | 验证应用结果与预期是否一致 |
| `health` | `&DetectionContext` | `AdapterHealthReport` | Adapter 自身健康检查 |

关键返回类型：

- `RenderOutput`：包含 `Projection`（元数据）和 `Vec<RenderedObject>`（待写入对象库的字节，只含资产内容或 `SecretRef`，不含物化 secret）。
- `AdapterPlan`：包含 `projection`、`remove_targets`、`removals`（精确移除托管块/结构化条目）和 `manual_required`（人工步骤）。

声明式检测、扫描、编解码、托管块和结构化条目等通用能力已由 SDK 提供，多数 Adapter 只需声明 manifest 并组合 SDK 函数，无需从零实现。

### 4. validate

```bash
rigdeck adapter validate ./adapters/my-agent
```

校验内容：manifest JSON Schema、跨字段语义、路径边界、symlink 策略和包大小限制（单文件 ≤16 MiB，总 ≤64 MiB，文件数 ≤1000，深度 ≤32）。

### 5. test

```bash
rigdeck adapter test ./adapters/my-agent
```

检查 `fixtures/windows` 和 `fixtures/macos` 的 home/project 夹具目录，并在当前平台运行声明式检测。检测必须至少命中一个实例，否则失败。

### 6. pack

```bash
rigdeck adapter pack ./adapters/my-agent
```

生成内容确定的 `.rigdeck-adapter` JSON bundle，内含按相对路径排序的文件、单文件 Blake3 hash、Base64 内容和整体 `package_hash`。同一输入重复打包产物逐字节一致。

## Surface 和投影模式

Surface 是资产作用域到本地路径的声明式映射。`mode` 字段决定 Core 如何在目标上保留用户内容。

| 模式 | 含义 | 典型用途 |
|---|---|---|
| `replace_file` | 目标文件完全由该资产拥有，可整体替换 | 独占的 prompt 文件、单文件配置 |
| `directory_tree` | 目标是一个 Skill 目录，资产清单决定目录内文件 | `skills/<name>/SKILL.md` 及附带文件 |
| `managed_block` | 只管理 Markdown/文本文件中的带边界托管块，用户其余内容不动 | `CLAUDE.md`、`AGENTS.md`、`SOUL.md` |
| `structured_entry` | 只管理 JSONC/TOML/YAML 配置中的一个结构化条目，保留注释和未知字段 | `opencode.jsonc`、`config.toml`、`config.yaml` 的 `mcp_servers` |
| `manual_required` | 官方没有可写本地接口，只生成交接文件并要求人工完成 | 云端配置、需浏览器操作的资产 |

规则：

- 共享文本（`managed_block`）使用唯一边界块，带资产 ID 和修订 hash；卸载只删除对应资产块。
- 结构化配置（`structured_entry`）只替换所属条目，无法无损解析时停止并报告冲突。
- `writable: false` 的 surface 仅用于发现和导入，不能作为新写入目标（如旧兼容路径）。
- `precedence` 决定冲突优先级：同一资产出现在多个 surface 时，数字小的优先。

## 安全约束

Adapter 运行在不可信边界，以下规则由 SDK 和 Core 强制：

1. **不直接写文件**。Adapter 只返回 `Projection` 和 `AdapterPlan`；所有写操作由 Core Planner 在批准计划内完成。
2. **路径边界**。所有检测路径和 surface 根必须以 `{home}` 或 `{project}` 开头；展开后 canonicalize 并校验位于允许根目录内。`target` 只允许 `{name}` 变量，拒绝绝对路径、`..` 和未知变量。
3. **symlink 拒绝**。`adapter.json` 不能是 symlink；包内任何 symlink 在 validate/pack 阶段被拒绝；检测路径通过 symlink 逃逸允许根目录时报 `PathViolation`。
4. **helper 信任**。声明 helper 不会自动执行；运行前须取得绑定 `adapter_id + hash` 的显式信任决定，更新后重新确认。
5. **secret 隔离**。`RenderedObject` 只含资产内容或 `SecretRef`，不含物化 secret；明文凭据仅在授权边界内短暂存在，永不出现在 SQLite、日志、计划或 JSON 输出。
6. **不支持的能**。无法自动完成的能力返回稳定的 `unsupported` 或 `manual_required`，附原因和恢复路径，不伪装成已支持。

## 发布流程

1. 完成开发并通过 `validate` 和 `test`。
2. `pack` 生成 `.rigdeck-adapter` bundle，确认 `package_hash` 稳定。
3. 若含 helper，记录 Blake3 hash 并准备权限披露文案，供用户做信任决定。
4. 将 bundle 分发到用户可达位置（本地目录、私有 registry 或仓库 release）。
5. 用户通过 RigDeck 注册 Adapter 包；Core 校验 manifest、协议兼容性和 helper 信任后加载。
6. 后续版本变更需更新 `version`；弃用时填写 `deprecation`，Core 会引导用户迁移到 `replacement`。

## 参考

- ADR-0004：[Adapter Contract](adr/0004-adapter-contract.md)
- ADR-0008：[Security Model](adr/0008-security.md)
- [适配器能力矩阵](adapters/capability-matrix.md)
- [CLI 参考 - Adapter 开发](cli-reference.md#adapter-开发)
- JSON Schema：[`adapter.schema.json`](../crates/rigdeck-adapter-sdk/schema/adapter.schema.json)
