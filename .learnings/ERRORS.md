# 工程错误记录

## [ERR-20260711-001] cargo 基线验证

**Logged**: 2026-07-11T10:46:00+08:00  
**Priority**: high  
**Status**: resolved  
**Area**: infra

### 摘要

首次执行 Rust 基线验证时，当前 Windows 环境找不到 `cargo`。

### 错误

```text
The term 'cargo' is not recognized as a name of a cmdlet, function, script file, or executable program.
```

### 上下文

- 尝试命令：`cargo fmt --all -- --check`、`cargo test --workspace`
- 当前 shell：PowerShell
- `%USERPROFILE%\.cargo\bin` 与 `%USERPROFILE%\.rustup\toolchains` 均不存在
- 仓库分支：`codex/complete-rigdeck-ga`

### 建议修复

确认 Codex 工作区是否提供捆绑 Rust；若没有，安装并固定受支持的 Rust 工具链，然后重新执行格式化、测试和 Clippy 基线。

### 元数据

- Reproducible: yes
- Related Files: `Cargo.toml`, `AGENTS.md`

### 解决结果

- **Resolved**: 2026-07-11T10:56:00+08:00
- **Notes**: 安装 Rustup 1.29.0 与 `stable-msvc` 1.97.0，并验证 Cargo、rustfmt、Clippy 可用。本机原生链接仍需 Microsoft C++ Build Tools，已单独列为环境门禁。

---

## [ERR-20260712-049] watcher 测试不能假设首批事件就是最终文件路径

**Logged**: 2026-07-12T02:30:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: watcher

### 摘要

目录创建、文件创建和重命名在不同平台会拆成不同顺序的事件；首批事件可能只包含父目录。删除旧路径时，如果同 hash 的重命名目标仍存在，刷新也会正确归类为 rename，而不是 remove。

### 解决结果

- watcher 事件只作为“需要重新扫描”的失效提示，产品逻辑不从单条事件推断最终状态。
- 端到端测试断言 2 秒内完成可信全量刷新及最终分类，不绑定平台特定事件序列。
- 删除用例先清理重命名候选，避免 rename 语义掩盖真正的 external removal。

---

## [ERR-20260712-045] Windows 受限沙箱无法创建新进程

**Logged**: 2026-07-12T01:10:00+08:00  
**Priority**: medium  
**Status**: open  
**Area**: tooling

### 摘要

受限沙箱在已完成多轮测试后持续于 `CreateProcessAsUserW` 返回 Windows error 5，连只读 `cargo check` 和端口查询也无法启动。

### 解决结果

- 已停止本轮自行启动的 Vite 预览服务，故障仍存在。
- 沙箱外 Windows 环境缺少 MSVC linker，最终使用 WSL Rust 工具链完成编译、测试和 clippy；正式 Windows 构建继续由 Windows CI 验证。

---

## [ERR-20260712-046] npm exec 参数分隔导致 Prettier 只打印文件

**Logged**: 2026-07-12T01:25:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: frontend-tooling

### 摘要

`npm exec prettier -- --write ...` 在当前 npm 版本把 `--write` 解释为 npm 配置，Prettier 只把文件内容输出到终端。

### 解决结果

改用 `npx prettier --write ...`，随后完整执行 TypeScript、i18n 和 Vite 生产构建门禁。

---

## [ERR-20260712-047] cargo test 一次只能接受一个测试过滤串

**Logged**: 2026-07-12T01:42:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: rust-testing

### 摘要

向一次 `cargo test` 传入两个独立测试名时，第二个名称被识别为意外参数。

### 解决结果

需要多个精确测试时分别运行，或运行所属 crate 的完整测试套件；不得把多个名称当作并列过滤器。

---

## [ERR-20260712-048] 无操作冲突计划仍需建立新基线

**Logged**: 2026-07-12T01:48:00+08:00  
**Priority**: high  
**Status**: resolved  
**Area**: conflict-resolution

### 摘要

三方合并结果恰好等于 Agent 当前文件时，`Planner::write_file` 正确省略了无效写入，但也导致计划没有快照目标；若直接标记解决，下次刷新会重新打开同一冲突。

### 解决结果

三方合并或逐文件人工结果等于当前文件时改为显式 `AdoptBaseline`，不改字节但校验 hash 并产生稳定新基线；新增参数化动作测试覆盖该分支。

---

## [ERR-20260711-041] npm 镜像站缺少安全审计端点

**Logged**: 2026-07-11T21:54:00+08:00  
**Priority**: high  
**Status**: resolved  
**Area**: supply-chain

### 摘要

本机 npm 默认 registry 指向镜像站；该镜像可以安装依赖，但没有实现 npm 安全公告 API，导致 `npm audit` 以 404 失败。

### 错误

```text
[NOT_IMPLEMENTED] /-/npm/v1/security/* not implemented yet
```

### 建议修复

安全审计必须显式使用 npm 官方 registry，不能把第三方镜像的接口缺失解释为“无漏洞”；CI 也应固定审计端点。

### 解决结果

- **Resolved**: 2026-07-11T21:55:00+08:00
- **Notes**: 改用 `npm audit --registry=https://registry.npmjs.org`，并把显式端点写入 CI。

---

## [ERR-20260711-006] Codex Manual helper 返回 403

**Logged**: 2026-07-11T12:02:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: docs

### 摘要

`openai-docs` Skill 的 Codex Manual helper 对官方 manual 执行 HEAD 时返回 HTTP 403。

### 错误

```text
HEAD https://developers.openai.com/codex/codex-manual.md failed with HTTP 403
```

### 上下文

- 目标：核验 Codex Skills、AGENTS.md 与 MCP config 当前官方行为
- Helper 已实际执行，按 Skill 路由转用 OpenAI Developer Docs MCP

### 建议修复

本次使用 Docs MCP；后续 helper 应对官方站点禁用 HEAD 的情况回退 GET。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-adapters/`, `docs/`

### 解决结果

- **Resolved**: 2026-07-11T12:02:00+08:00
- **Notes**: 已按 Skill 采用下一官方来源，不阻塞实现。

---

## [ERR-20260711-005] Refinery 损坏 checksum 触发 panic

**Logged**: 2026-07-11T11:56:00+08:00  
**Priority**: high  
**Status**: resolved  
**Area**: backend

### 摘要

数据库恢复测试发现 Refinery 0.8.16 解析非法 migration checksum 时直接 panic，绕过正常 `Result` 错误与保护副本恢复。

### 错误

```text
checksum must be a valid u64: ParseIntError { kind: InvalidDigit }
```

### 上下文

- 测试：`database::tests::failed_migration_restores_pre_migration_database`
- 风险：损坏/恶意 SQLite 可导致桌面或 CLI 进程崩溃

### 建议修复

在 migration runner 边界捕获 unwind，将第三方 panic 转为 `StoreError::Integrity`，随后恢复一致性 backup。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-store/src/database.rs`

### 解决结果

- **Resolved**: 2026-07-11T11:57:00+08:00
- **Notes**: 已使用 `catch_unwind(AssertUnwindSafe(...))` 建立 panic 边界，并保留恢复测试。

---

## [ERR-20260711-004] Rust GNU 本地验证工具链安装超时

**Logged**: 2026-07-11T11:27:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: infra

### 摘要

为绕开本机缺少 Microsoft C++ Build Tools 的链接门禁，安装 Rust 官方 `stable-gnu` 工具链在 10 分钟后超时。

### 错误

```text
command timed out after 604068 milliseconds
```

### 上下文

- 默认 `stable-msvc` 仍完整可用
- `rust-lld` 已证明缺少 Windows SDK import libraries，不能单独替代 Build Tools
- GNU 工具链只用于本地 Core 验证，不用于 Tauri 正式工件

### 建议修复

检查孤儿 Rustup 进程是否仍有实际进展；停滞时移除不完整 GNU 工具链，保留 MSVC CI 作为正式验证路径。

### 元数据

- Reproducible: unknown
- Related Files: `.github/workflows/ci.yml`, `.agent/STATE.json`

### 解决结果

- **Resolved**: 2026-07-11T11:35:00+08:00
- **Notes**: 已卸载不完整 Windows GNU 工具链；改用现有 WSL2 Ubuntu 安装 Rust 1.88 与 `build-essential`，成功完成 `rigdeck-core --locked` 编译。正式 Windows 工件仍保留 MSVC CI 门禁。

---

## [ERR-20260711-003] 品牌 SVG 位图转换器探测

**Logged**: 2026-07-11T11:07:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: infra

### 摘要

Codex 捆绑 Python 环境未安装 `cairosvg`，无法直接把品牌 SVG 转为 PNG。

### 错误

```text
ModuleNotFoundError: No module named 'cairosvg'
```

### 上下文

- 目标：从唯一 SVG master 机械导出 PNG、ICO 与 ICNS
- Python：Codex workspace dependency runtime

### 建议修复

优先使用系统现有 SVG 转换器；若不存在，使用锁定的项目构建依赖完成可复现转换。

### 元数据

- Reproducible: yes
- Related Files: `docs/brand/assets/rigdeck-mark.svg`

### 解决结果

- **Resolved**: 2026-07-11T11:10:00+08:00
- **Notes**: 锁定 `@resvg/resvg-js` 生成 PNG，使用 Pillow 从 1024px master 生成 ICO/ICNS；16px、1024px 与社交预览均已视觉检查。

---

## [ERR-20260711-002] smart-search Gemini 官方资料检索

**Logged**: 2026-07-11T10:49:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: infra

### 摘要

通过 OpenCLI Gemini 检索 Rust 官方安装资料时，Browser Bridge 未连接。

### 错误

```text
BROWSER_CONNECT: Browser Bridge extension not connected
```

### 上下文

- 已按 `smart-search` 完成 `opencli list -f yaml`、`opencli gemini -h` 和 `opencli gemini ask -h` 预检
- Gemini 本题真实调用 1 次并失败，依据频率限制不再重试

### 建议修复

本次改用官方 Rust/Microsoft 网页只读核验；只有后续确需浏览器登录态搜索时才配置 Browser Bridge。

### 元数据

- Reproducible: yes
- Related Files: `.learnings/ERRORS.md`

### 解决结果

- **Resolved**: 2026-07-11T10:49:00+08:00
- **Notes**: 已采用官方网页检索替代，不阻塞工程任务。

---
## [ERR-20260711-007] WSL Rust 验证用户不一致

**Logged**: 2026-07-11T13:50:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: infra

### 摘要

以 `root` 身份运行 WSL 验证时未找到先前安装在普通用户环境中的 `cargo`。

### 错误

```text
bash: line 1: cargo: command not found
```

### 上下文

- 失败命令：`wsl -u root -- bash -lc "... cargo test ..."`
- WSL 默认用户为 `hpli`；Rust 工具链此前按用户安装
- 系统包由 root 安装不代表 Rustup 工具链也安装在 root HOME

### 建议修复

固定使用 WSL 默认用户运行 Cargo，仅在安装系统包时使用 root；把该约定写入项目验证脚本。

### 元数据

- Reproducible: yes
- Related Files: `scripts/check-rust.ps1`, `.github/workflows/ci.yml`

### 解决结果

- **Resolved**: 2026-07-11T13:53:00+08:00
- **Notes**: 已确认工具链位于 `/home/hpli/.cargo/bin`；后续 Cargo 命令固定使用 `wsl -u hpli` 并显式补充 PATH，`rigdeck-adapters` 验证恢复通过。

---
## [ERR-20260711-008] Adapter 扫描去重类型缺少排序能力

**Logged**: 2026-07-11T14:25:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: backend

### 摘要

声明式 Adapter 扫描用 `BTreeSet<(Utf8PathBuf, AssetKind)>` 去重，但 `AssetKind` 尚未实现 `Ord`。

### 错误

```text
error[E0277]: the trait bound `AssetKind: Ord` is not satisfied
```

### 上下文

- 发生于 `rigdeck-adapters` 第一轮编译
- `AssetKind` 是无负载稳定枚举，定义确定性排序不会改变领域语义

### 建议修复

为 `AssetKind` 派生 `PartialOrd`、`Ord` 和 `Hash`，同时支持确定性集合及 hash 集合。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-core/src/model.rs`, `crates/rigdeck-adapters/src/builtin.rs`

### 解决结果

- **Resolved**: 2026-07-11T14:26:00+08:00
- **Notes**: 已补充稳定排序与 hash 派生，继续执行全套契约测试。

---
## [ERR-20260711-009] Adapter 测试缺少 tempfile 开发依赖

**Logged**: 2026-07-11T14:27:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: tests

### 摘要

隔离 HOME/项目夹具使用了 `tempfile`，但新 Adapter crate 尚未声明对应开发依赖。

### 错误

```text
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `tempfile`
```

### 上下文

- 生产代码不依赖 `tempfile`
- 仅黄金夹具和路径安全测试需要临时目录

### 建议修复

在 `[dev-dependencies]` 中复用 workspace 锁定的 `tempfile`。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-adapters/Cargo.toml`

### 解决结果

- **Resolved**: 2026-07-11T14:28:00+08:00
- **Notes**: 已作为纯测试依赖补充，不进入生产依赖表面。

---
## [ERR-20260711-010] YAML 对象条目首行缩进错误

**Logged**: 2026-07-11T14:45:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: backend

### 摘要

首次 YAML 定点补丁把对象首字段错误地内联到父 key 后，生成 `demo: command: demo`。

### 错误

```text
YAML 解析失败：mapping values are not allowed in this context at line 6 column 13
```

### 上下文

- 标量可以写成 `key: value`
- 对象和数组必须换行后整体增加子级缩进
- 补丁器的写后语法验证正确阻止了无效内容落盘

### 建议修复

按 JSON payload 类型区分标量与容器；对象/数组始终使用块样式。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-adapters/src/structured.rs`

### 解决结果

- **Resolved**: 2026-07-11T14:46:00+08:00
- **Notes**: 已改为容器块缩进，并保留写后 `serde_yaml` 验证门禁。

---
## [ERR-20260711-011] WSL 挂载盘全工作区测试超时

**Logged**: 2026-07-11T15:05:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: infra

### 摘要

一次性运行除桌面外的全工作区测试，在 `/mnt/c` 上编译超过 180 秒被外层命令终止。

### 错误

```text
command timed out after 184160 milliseconds
```

### 上下文

- 单 crate 测试此前均通过
- WSL 访问 Windows NTFS 挂载盘的小文件编译 I/O 明显慢于 Linux 文件系统
- 超时没有输出 Rust 编译错误

### 建议修复

开发循环使用 crate 级增量测试；里程碑门禁再使用更长超时的 workspace 检查，正式矩阵交给原生 CI。

### 元数据

- Reproducible: unknown
- Related Files: `.github/workflows/ci.yml`

### 解决结果

- **Resolved**: 2026-07-11T15:06:00+08:00
- **Notes**: 已切换为 crate 级测试和严格 Clippy；最终门禁保留全工作区命令与 CI。

---
## [ERR-20260711-012] Watcher 模块残留未使用导入

**Logged**: 2026-07-11T15:25:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: backend

### 摘要

Watcher 测试全部通过，但严格 Clippy 因残留 `Utf8Path` 导入而失败。

### 错误

```text
error: unused import: `Utf8Path`
```

### 上下文

- `-D warnings` 正确把清洁度问题提升为门禁
- 运行时代码与测试行为均未受影响

### 建议修复

移除未使用导入并重跑严格 Clippy。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-adapters/src/watch.rs`

### 解决结果

- **Resolved**: 2026-07-11T15:26:00+08:00
- **Notes**: 已移除导入。

---
## [ERR-20260711-013] 里程碑状态文件名假设错误

**Logged**: 2026-07-11T15:45:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: docs

### 摘要

同步里程碑时尝试读取不存在的 `.agent/STATE.md`；项目实际使用 `.agent/STATE.json`。

### 错误

```text
Cannot find path '.agent/STATE.md' because it does not exist.
```

### 上下文

- `.agent/PROGRESS.md` 和 `.agent/CONTEXT.md` 是 Markdown
- 机器可读续接状态从一开始就是 `.agent/STATE.json`

### 建议修复

后续状态同步只读取并更新 `STATE.json`，不创建重复事实源。

### 元数据

- Reproducible: yes
- Related Files: `.agent/STATE.json`

### 解决结果

- **Resolved**: 2026-07-11T15:46:00+08:00
- **Notes**: 已按实际文件更新 M2 阶段状态。

---
## [ERR-20260711-014] CLI 未导入 AgentAdapter trait

**Logged**: 2026-07-11T16:10:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: backend

### 摘要

CLI 调用 `BuiltinAdapter::describe()` 时没有把提供该方法的 `AgentAdapter` trait 引入作用域。

### 错误

```text
error[E0599]: no method named `describe` found for struct `BuiltinAdapter`
```

### 上下文

Rust 的 trait 方法只有在 trait 可见时才能用方法语法调用；这不是继承式隐式查找。

### 建议修复

显式 `use rigdeck_adapter_sdk::AgentAdapter`，并清理占位命令的未使用变量。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-cli/src/main.rs`

### 解决结果

- **Resolved**: 2026-07-11T16:11:00+08:00
- **Notes**: 已导入 trait 并消除同轮编译警告。

---
## [ERR-20260711-015] Registry 严格 Clippy 风格门禁

**Logged**: 2026-07-11T16:55:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: backend

### 摘要

Registry 功能测试通过，但严格 Clippy 要求折叠 URL 条件并使用切片 `contains`。

### 错误

```text
clippy::collapsible-if
clippy::manual-contains
```

### 上下文

- 不影响运行语义
- `-D warnings` 正确保持所有 crate 的代码清洁门禁一致

### 建议修复

采用 Clippy 建议的等价写法并重跑全部 Registry 门禁。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-registry/src/http.rs`, `crates/rigdeck-registry/src/importer.rs`

### 解决结果

- **Resolved**: 2026-07-11T16:56:00+08:00
- **Notes**: 已完成等价改写。

---
## [ERR-20260711-016] PowerShell 未找到 Cargo 时组合命令误报成功

**Logged**: 2026-07-11T17:23:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: tooling

### 摘要

在 Windows PowerShell 中直接执行 `cargo fmt` 时，系统找不到 Cargo；后续条件判断没有可靠捕获“命令不存在”，导致整个组合命令最终返回成功。

### 错误

```text
The term 'cargo' is not recognized as a name of a cmdlet, function, script file, or executable program.
```

### 上下文

本项目当前已验证的 Rust 工具链位于 WSL 用户 `hpli` 的 `$HOME/.cargo/bin`。PowerShell 的 `$LASTEXITCODE` 只适合检查已启动的原生程序，命令解析失败时不能据此可靠判断。

### 建议修复

所有本地 Rust 门禁统一通过 WSL 执行，并在 Bash 内用 `&&` 串联，使任一阶段失败都会产生非零退出码。

### 元数据

- Reproducible: yes
- Related Files: `.learnings/ERRORS.md`

### 解决结果

- **Resolved**: 2026-07-11T17:24:00+08:00
- **Notes**: 已在 WSL 中重新执行 `cargo fmt --all -- --check` 与严格 Clippy，二者均通过。

---
## [ERR-20260711-017] CLI 接线时错误假定 ApplyReport 字段名

**Logged**: 2026-07-11T17:41:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: backend

### 摘要

CLI 人类输出把 `ApplyReport` 中的应用结果数量误写成不存在的 `applied` 字段；实际稳定模型使用 `files: Vec<AppliedFile>`。

### 错误

```text
error[E0609]: no field `applied` on type `ApplyReport`
```

### 上下文

Rust 编译器在共享服务接线阶段验证了 CLI 对 Core 数据合同的假设，同时报告一个未使用的 `Utf8Path` 导入。

### 建议修复

使用 `report.files.len()` 计算完成操作数，并删除未使用导入。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-cli/src/main.rs`

### 解决结果

- **Resolved**: 2026-07-11T17:42:00+08:00
- **Notes**: 已按 Core 真实类型修正，随后重新运行编译门禁。

---
## [ERR-20260711-018] Tauri 首次 WSL 慢盘检查超时

**Logged**: 2026-07-11T18:16:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: tooling

### 摘要

在 WSL 中从 Windows 挂载目录 `/mnt/c` 首次执行 `cargo check -p rigdeck-desktop`，244 秒内未完成且没有返回编译诊断。

### 错误

```text
command timed out after 244066 milliseconds
```

### 上下文

Tauri/WebKit 依赖图较大，跨文件系统元数据与增量产物写入明显慢于 Linux 原生目录；此前 workspace 级构建也有相同特征。

### 建议修复

保留首次运行产生的增量产物，随后单独重跑目标 crate；CI 的 Windows/macOS 原生 runner 继续作为发布级证据。

### 元数据

- Reproducible: yes
- Related Files: `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/src/lib.rs`

### 解决结果

- **Resolved**: 2026-07-11T18:17:00+08:00
- **Notes**: 已改为使用增量产物单独复跑桌面检查。

---
## [ERR-20260711-019] 非发布目标 Linux 缺少 D-Bus 开发库

**Logged**: 2026-07-11T18:20:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: tooling

### 摘要

WSL/Linux 检查桌面壳时，keyring 的 Linux 后端依赖 `dbus-1.pc`，当前环境没有安装 `libdbus-1-dev`。

### 错误

```text
Package 'dbus-1', required by 'virtual:world', not found
```

### 上下文

RigDeck 当前发布范围是 Windows 与 macOS，Linux 不是交付目标。为一次非目标平台检查安装系统级 D-Bus/GTK/WebKit 开发包会扩大环境修改范围，也不能替代目标平台验证。

### 建议修复

在 WSL 安装 Rust 的 Windows MSVC 标准库目标做交叉 `cargo check`；发布证据继续由 Windows/macOS CI 提供。

### 元数据

- Reproducible: yes
- Related Files: `crates/rigdeck-security/Cargo.toml`, `apps/desktop/src-tauri/Cargo.toml`

### 解决结果

- **Resolved**: 2026-07-11T18:21:00+08:00
- **Notes**: 不修改非目标系统包；交叉检查随后又暴露宿主缺少 MSVC 工具，最终以原生 CI 为发布证据。

---
## [ERR-20260711-020] WSL 无法交叉构建 ring 的 MSVC C/ASM 代码

**Logged**: 2026-07-11T18:25:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: tooling

### 摘要

安装 `x86_64-pc-windows-msvc` Rust 标准库后，`ring` 的 build script 仍需要 Visual Studio 的 `lib.exe`，WSL 宿主没有该专有工具链。

### 错误

```text
failed to find tool "lib.exe": No such file or directory
```

### 上下文

`cargo check` 也会执行依赖 build script；仅安装 Rust target 不能替代 MSVC C/ASM 工具。Windows/macOS 原生 CI 已覆盖目标平台。

### 建议修复

本地继续验证全部平台无关 crate 与 TypeScript；Tauri 壳由目标平台 CI 编译。不要为了交叉检查引入未经确认的 Visual Studio 许可或系统级 Linux GUI 依赖。

### 元数据

- Reproducible: yes
- Related Files: `.github/workflows/ci.yml`, `apps/desktop/src-tauri/Cargo.toml`

### 解决结果

- **Resolved**: 2026-07-11T18:26:00+08:00
- **Notes**: 已把验证职责明确分为本地平台无关门禁与 Windows/macOS 原生 CI。

---
## [ERR-20260711-021] 前端首轮严格 TypeScript 门禁

**Logged**: 2026-07-11T18:58:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: frontend

### 摘要

七页桌面 UI 首轮 `tsc --noEmit` 报告两个未使用导入、ES target 不支持 `String.replaceAll`，以及替换回调的隐式 `any`。

### 错误

```text
TS6133 / TS6196 / TS2550 / TS7006
```

### 上下文

项目启用严格 TypeScript 与未使用项检查；这批问题不涉及运行时结构或 IPC 参数合同。

### 建议修复

清理导入，使用 ES2019 兼容的全局正则替换，并显式标注回调参数类型。

### 元数据

- Reproducible: yes
- Related Files: `apps/desktop/src/App.tsx`

### 解决结果

- **Resolved**: 2026-07-11T18:59:00+08:00
- **Notes**: 已按现有 tsconfig target 改写，并补充启动主题恢复。

---
## [ERR-20260711-022] 浏览器驱动不支持 networkidle 等待状态

**Logged**: 2026-07-11T19:04:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: testing

### 摘要

本地桌面验收首次尝试使用 `networkidle` 等待状态，但当前内置浏览器驱动明确不支持该值。

### 错误

```text
playwright_wait_for_load_state does not support networkidle
```

### 上下文

Vite 预览是纯本地静态页面，后续 DOM snapshot 已能可靠证明首屏就绪，不需要网络空闲语义。

### 建议修复

导航后直接读取 DOM snapshot，并对关键 heading/控件使用可见性等待。

### 元数据

- Reproducible: yes
- Related Files: `apps/desktop/src/App.tsx`

### 解决结果

- **Resolved**: 2026-07-11T19:05:00+08:00
- **Notes**: 已改用 DOM 与关键元素可见性作为浏览器验收信号。

---
## [ERR-20260711-023] Cargo test 误传两个位置过滤串

**Logged**: 2026-07-11T19:28:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: testing

### 摘要

运行 Windows 与 macOS 黄金测试时把两个测试名都作为位置参数传给 `cargo test`，但 Cargo 只接受一个 `TESTNAME` 过滤串。

### 错误

```text
error: unexpected argument 'macos_golden_fixtures_cover_all_seven_adapters' found
```

### 建议修复

使用两个测试共同包含的 `golden_fixtures_cover_all_seven_adapters` 作为单一过滤串。

### 解决结果

- **Resolved**: 2026-07-11T19:29:00+08:00
- **Notes**: 已改为一个公共过滤串。

---
## [ERR-20260711-024] 多实例修正后旧测试仍跨 profile 渲染

**Logged**: 2026-07-11T19:36:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: testing

### 摘要

Adapter 全量测试中两条 Codex 测试仍取检测结果的第一个全局实例，却请求 `project` scope；新合同会正确返回 `UnsupportedCapability`。

### 错误

```text
没有 Prompt/project 的投影 surface
没有可写 Skill directory surface
```

### 建议修复

夹具创建项目 marker，并按 `profile == project` 显式选择项目实例，禁止依赖检测顺序。

### 解决结果

- **Resolved**: 2026-07-11T19:37:00+08:00
- **Notes**: 已更新两条旧测试以遵循 profile 边界。

---
## [ERR-20260711-025] Pi 当前包要求更高 Node 22 patch 版本

**Logged**: 2026-07-11T20:02:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: packaging

### 摘要

安装 Pi companion extension 的开发依赖时，官方 `@earendil-works/pi-coding-agent@0.80.6` 要求 Node `>=22.19.0`，本机为 22.14.0。

### 错误

```text
npm warn EBADENGINE required: { node: '>=22.19.0' }, current: v22.14.0
```

### 建议修复

扩展 package engine 与官方 Pi 保持一致；当前环境只运行不依赖 Pi 启动的静态类型和配置安全测试，发布 CI 使用满足版本的 Node。

### 解决结果

- **Resolved**: 2026-07-11T20:03:00+08:00
- **Notes**: 已把扩展 engine 提升为 `>=22.19.0`。

---
## [ERR-20260711-026] 组合式 PowerShell 读取再次被 Windows 沙箱拒绝

**Logged**: 2026-07-11T20:24:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: tooling

### 摘要

使用单个 PowerShell 进程串联两个 `Get-Content | Select-Object` 只读命令时，受管沙箱在创建子进程阶段返回访问拒绝。

### 错误

```text
CreateProcessAsUserW failed: 5 (拒绝访问。)
```

### 建议修复

优先使用已批准的单段 `rg -n -C` 或拆分后的 `Get-Content`，避免在当前 Windows 受管环境中组合多个管道命令。

### 解决结果

- **Resolved**: 2026-07-11T20:25:00+08:00
- **Notes**: 已改用单段 `rg` 完成源码核验。

---
## [ERR-20260711-027] 新增 Rust 表驱动测试未先执行 rustfmt

**Logged**: 2026-07-11T20:28:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: testing

### 摘要

严格回归命令先执行 `cargo fmt --check`，发现上一轮补入的冲突分类表测试仍是未格式化状态，因此测试链在编译前按设计停止。

### 错误

```text
Diff in crates/rigdeck-core/src/refresh.rs
```

### 建议修复

每次通过补丁新增 Rust 多行结构体或元组表后，先执行 `cargo fmt --all`，再运行 `cargo fmt --check` 和测试。

### 解决结果

- **Resolved**: 2026-07-11T20:29:00+08:00
- **Notes**: 已执行 rustfmt，继续完整测试链。

---
## [ERR-20260711-028] 生命周期测试重复使用已移动的 Utf8PathBuf

**Logged**: 2026-07-11T20:33:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: testing

### 摘要

生命周期测试第一次调用 `refresh(home, None)` 时把 `Utf8PathBuf` 的所有权移入函数，新增的第二次刷新又尝试使用同一变量，触发 Rust `E0382`。

### 错误

```text
error[E0382]: use of moved value: `home`
```

### 建议修复

若测试后续仍需复用路径，在第一次按值传参时显式调用 `home.clone()`；生产 API 继续保持清晰的所有权边界。

### 解决结果

- **Resolved**: 2026-07-11T20:34:00+08:00
- **Notes**: 首次刷新改为传入路径克隆，并加入中文所有权说明。

---
## [ERR-20260711-029] 带管道的 Get-Content 再次触发沙箱进程拒绝

**Logged**: 2026-07-11T20:42:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: tooling

### 摘要

使用 `Get-Content ... | Select-Object` 读取 Planner 局部源码时，Windows 受管沙箱再次在子进程创建阶段返回访问拒绝。

### 错误

```text
CreateProcessAsUserW failed: 5 (拒绝访问。)
```

### 建议修复

在该环境中避免 PowerShell 读取管道，统一使用单段 `rg -n -C` 获取带上下文的源码位置。

### 解决结果

- **Resolved**: 2026-07-11T20:43:00+08:00
- **Notes**: 已改用 `rg -n -C 8 "pub fn"` 完成接口核验。

---
## [ERR-20260711-030] Doctor 检查项逐个 push 触发严格 Clippy

**Logged**: 2026-07-11T20:48:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: lint

### 摘要

Doctor 连续向新建 `Vec` 推入七个固定检查项，严格 `-D warnings` 将 `vec_init_then_push` 提升为错误。

### 错误

```text
error: calls to `push` immediately after creation
```

### 建议修复

固定长度的初始化项直接使用 `vec![...]`；只有条件分支或后续动态追加时才保留可变向量。

### 解决结果

- **Resolved**: 2026-07-11T20:49:00+08:00
- **Notes**: Doctor 改为一次性 `vec!` 初始化，不添加 lint 豁免。

---
## [ERR-20260711-031] PowerShell 检索字符串包含反引号导致启动失败

**Logged**: 2026-07-11T20:52:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: tooling

### 摘要

用于检索 TODO 的 PowerShell 命令把 Markdown 反引号放进双引号参数，反引号被 PowerShell 当作转义符处理，受管运行器最终拒绝启动。

### 错误

```text
CreateProcessAsUserW failed: 5 (拒绝访问。)
```

### 建议修复

shell 检索模式不包含反引号、`$()` 等会被 shell 解释的字符；改用足够唯一的普通文本片段。

### 解决结果

- **Resolved**: 2026-07-11T20:53:00+08:00
- **Notes**: 已用无特殊字符的 `rg` 模式完成定位。

---
## [ERR-20260711-032] 单文件大小错误消息未使用内联格式参数

**Logged**: 2026-07-11T21:08:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: lint

### 摘要

单文件资产大小上限错误消息使用旧式格式参数，严格 Clippy 的 `uninlined_format_args` 将其提升为错误。

### 错误

```text
error: variables can be used directly in the `format!` string
```

### 建议修复

Rust 1.58+ 直接在格式串中捕获作用域内变量或常量，如 `{MAX_SINGLE_ASSET_BYTES}`。

### 解决结果

- **Resolved**: 2026-07-11T21:09:00+08:00
- **Notes**: 已改为内联格式参数，未添加 lint 豁免。

---
## [ERR-20260711-033] 多文件补丁因后续上下文不匹配返回部分应用

**Logged**: 2026-07-11T21:10:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: tooling

### 摘要

同时修改源码与学习日志的补丁报告找不到源码旧上下文，但核验发现源码片段已经变更、日志尚未追加，表现为部分应用。

### 错误

```text
Failed to find expected lines in crates/rigdeck-service/src/lib.rs
```

### 建议修复

补丁失败后逐个核验所有目标文件，不假定整体原子；对未应用的目标使用更小且基于当前内容的补丁。

### 解决结果

- **Resolved**: 2026-07-11T21:11:00+08:00
- **Notes**: 已确认源码正确，并单独补写两条学习记录。

---
## [ERR-20260711-034] 确定性计划 ID 在停用启用循环后无法重新应用

**Logged**: 2026-07-11T21:36:00+08:00  
**Priority**: high  
**Status**: resolved  
**Area**: transaction

### 摘要

同一 Assignment 完成“停用 → 启用 → 再停用”后，第二个停用计划与第一个语义相同，因此 ID 相同；`save_plan` 的 `ON CONFLICT DO NOTHING` 保留了旧的 applied 状态，导致合法的新计划无法应用。

### 错误

```text
计划 ... 当前状态为 applied，只能应用 pending 计划
```

### 建议修复

保持语义确定性 ID；只有 Planner 显式重新生成并保存同一计划时，才把对应行重新置为 pending。直接重复 apply 不调用 `save_plan`，仍然失败关闭。

### 解决结果

- **Resolved**: 2026-07-11T21:38:00+08:00
- **Notes**: `save_plan` 改为冲突时更新 JSON/时间并重新打开 pending，生命周期测试覆盖循环场景。

---
## [ERR-20260711-035] 启动摘要样式补丁使用了过期 CSS 上下文

**Logged**: 2026-07-11T22:03:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: frontend

### 摘要

补丁预期 `.page-header p` 使用 `var(--text-muted)`，实际主题系统已改为 `rgb(var(--foreground-muted))`，导致补丁验证失败。

### 错误

```text
apply_patch verification failed: Failed to find expected lines in globals.css
```

### 建议修复

修改高频演进的样式文件前先用 `rg -n -C` 读取当前选择器，并沿用现有 RGB token 约定。

### 解决结果

- **Resolved**: 2026-07-11T22:04:00+08:00
- **Notes**: 已基于当前选择器重新补入启动摘要样式和双语文案。

---
## [ERR-20260711-036] AssetInspection 补丁使用了错误字段注释上下文

**Logged**: 2026-07-11T22:17:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: tooling

### 摘要

补丁预期 `AssetInspection.revision` 上方注释为“当前不可变修订”，实际源码是“修订”，导致验证失败。

### 错误

```text
apply_patch verification failed: Failed to find expected lines in rigdeck-service/src/lib.rs
```

### 建议修复

跨多个构造点扩展公开结构体前，先定位结构定义和全部字面量构造位置，再拆分为小补丁。

### 解决结果

- **Resolved**: 2026-07-11T22:18:00+08:00
- **Notes**: 已按当前定义和三个构造点重新应用。

---
## [ERR-20260711-037] Library 文案补丁匹配了错误的中文键值

**Logged**: 2026-07-11T22:24:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: frontend

### 摘要

补丁预期 i18n 值为“未验证命名空间”，实际为“命名空间未验证”，导致同批 CSS 变更也未应用。

### 错误

```text
apply_patch verification failed: Failed to find expected lines in i18n.ts
```

### 建议修复

中英文资源同时编辑前用稳定 key 定位，不依赖易变化的翻译值作为唯一上下文。

### 解决结果

- **Resolved**: 2026-07-11T22:25:00+08:00
- **Notes**: 已按 `verifiedNamespace/unverifiedNamespace` 当前块补入文案并独立补样式。

---
## [ERR-20260711-038] PowerShell 分号串联掩盖前端 i18n 门禁失败

**Logged**: 2026-07-11T22:31:00+08:00  
**Priority**: high  
**Status**: resolved  
**Area**: testing

### 摘要

`typecheck; lint:i18n; build` 中 i18n 正确发现 JSX 可见字面量，但 PowerShell 继续执行 build，并以最后一个命令的 0 作为整体退出码，可能制造门禁全绿假象。

### 错误

```text
发现未进入 i18n 的 JSX 可见字面量：App.tsx -> "B ·"
```

### 建议修复

提供单一 `npm run check`，内部用 `&&` 失败即停；自动化只调用这一条 npm script，不再用 PowerShell 分号拼接门禁。

### 解决结果

- **Resolved**: 2026-07-11T22:32:00+08:00
- **Notes**: 文件元数据改用双语插值键，并新增 fail-fast `check` script。

---
## [ERR-20260711-039] Activity 多文件补丁再次依赖了翻译值上下文

**Logged**: 2026-07-11T22:37:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: tooling

### 摘要

补丁预期恢复确认文案含“恢复前会先创建”，实际为“恢复会先创建”，导致 Activity JSX/CSS 同批变更未应用。

### 错误

```text
apply_patch verification failed: Failed to find expected lines in i18n.ts
```

### 建议修复

多文件补丁中的 i18n 变更使用稳定 key 和当前块作为锚点；长翻译值只作为替换目标时必须先精确读取。

### 解决结果

- **Resolved**: 2026-07-11T22:38:00+08:00
- **Notes**: 已按当前 `objectCount/restoreConfirm` 块补入详情键，并单独匹配现有 Activity JSX。

---
## [ERR-20260711-040] ZIP 累计大小变量被插入错误函数

**Logged**: 2026-07-11T22:48:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: security

### 摘要

归档安全补丁用通用 `let mut files` 作为上下文，把 `total` 插入 `read_local_directory`，而 `read_zip` 中使用时变量不存在。

### 错误

```text
error[E0425]: cannot find value `total` in this scope
```

### 建议修复

同一文件存在多个相同局部声明时，补丁必须包含函数签名或唯一相邻条件，避免命中第一个文本位置。

### 解决结果

- **Resolved**: 2026-07-11T22:49:00+08:00
- **Notes**: 已从本地目录函数移除，并放到 `read_zip` 的 archive 长度检查之后。

---

## [ERR-20260711-042] Vite 8 不再隐式提供 esbuild

**Logged**: 2026-07-11T22:02:00+08:00  
**Priority**: medium  
**Status**: resolved  
**Area**: frontend

### 摘要

为修复 Vite 高危漏洞升级到 8.1.4 后，生产构建仍使用 esbuild 转译路径，但新版 Vite 要求项目显式安装 esbuild。

### 错误

```text
Failed to load `transformWithEsbuild` ... requires esbuild to be installed separately
```

### 建议修复

升级主要构建工具后必须立即运行完整生产构建；若项目配置仍选择兼容转译器，应把该工具作为固定版本的直接开发依赖。

### 解决结果

- **Resolved**: 2026-07-11T22:03:00+08:00
- **Notes**: 固定增加 `esbuild@0.28.1`，随后重新执行类型检查、i18n 门禁、生产构建和漏洞审计。

---

## [ERR-20260712-043] CycloneDX 拆分模式与自定义文件名互斥

**Logged**: 2026-07-12T00:07:00+08:00  
**Priority**: low  
**Status**: resolved  
**Area**: supply-chain

### 摘要

`cargo-cyclonedx 0.5.9` 不允许同时使用 `--describe binaries` 与 `--override-filename`。

### 错误

```text
the argument '--describe <DESCRIBE>' cannot be used with '--override-filename <FILENAME>'
```

### 建议修复

需要稳定发布文件名时，为每个发布 crate 生成一份聚合 SBOM；该清单仍会把 Cargo targets 记录为子组件。

### 解决结果

- **Resolved**: 2026-07-12T00:08:00+08:00
- **Notes**: CLI 与桌面端分别生成独立的 crate 级 CycloneDX JSON，并核验元数据组件名。

---

## [ERR-20260712-044] actionlint 源码安装缺少 Go 工具链

**Logged**: 2026-07-12T00:39:00+08:00  
**Priority**: low  
**Status**: open  
**Area**: ci

### 摘要

尝试用官方建议的 `go install` 安装 actionlint 时，当前 WSL 环境没有 Go；随后下载官方预编译包又因 GitHub 连接被重置而失败。

### 错误

```text
go: command not found
curl: Failed to connect to github.com port 443
```

### 建议修复

项目 CI 应直接执行工作流，开发环境可选固定版本的 actionlint；本地没有 Go 或 GitHub 不可达时，不应为一次静态检查临时引入未经校验的下载源。

### 解决结果

- **Notes**: CI YAML 已人工核对，所有新增命令均在本机逐条实跑；待 GitHub 网络恢复后补跑 `actionlint`，不把未执行的静态检查声称为已验证。

---
