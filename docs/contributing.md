# 贡献指南

参与 RigDeck 开发前，先读 `AGENTS.md`、`docs/adr/` 和 `docs/security/threat-model.md`。中文是项目主要交付语言；代码符号、协议标识符和必要英文摘要保留英文。

## 开发环境要求

- Rust 1.88 或更高版本，组件：`rustfmt`、`clippy`。
- Node.js 20 LTS 与 npm。
- Windows：Microsoft C++ Build Tools、WebView2 Runtime。
- macOS：Xcode Command Line Tools。
- Linux：`webkit2gtk`、`libssl`、`librsvg` 等 Tauri 依赖。

验证环境：

```powershell
rustc --version   # 1.88+
node --version    # v20+
cargo --version
```

## 代码规范

- Rust：遵循 `rustfmt` 默认配置，`clippy` 零警告。关键所有权转移、生命周期、trait 边界写中文注释解释设计意图，不逐行翻译语法。公共 API 用中文 rustdoc，保留英文类型/字段名。优先小型值对象与穷尽 enum，禁止用字符串暗示状态机。`unsafe` 默认禁止，确需使用必须有 ADR、安全不变量注释和专门测试。
- TypeScript：strict 模式，禁用 `any`（除非附显式注释说明理由）。
- i18n：所有用户可见字符串进入 i18n 资源文件，不在代码中硬编码 UI 文案。运行 `npm run lint:i18n` 检查覆盖率。

## 架构硬规则

以下规则不可违反，违反即拒绝合并：

- **Core 无 Agent 分支**：`rigdeck-core` 不得按 Agent 名称写条件分支。Agent 差异全部封装在 Adapter 中。
- **Adapter 不写文件**：Adapter 只返回 `Projection` 和操作意图，文件写入由 Core 事务引擎统一执行。
- **前端不碰 SQLite**：前端不直连数据库或 Agent 文件，所有访问经 Tauri IPC → Rust Core。
- **secret 不持久化**：明文 secret 不得出现在 SQLite、日志、计划、JSON 输出、导出包中，统一走 `SecretRef` 引用钥匙串。
- **变更走 Planner**：任何写盘操作必须经 Plan → preview → backup → apply → verify → audit 流程。

## 提交前检查清单

每次提交前本地运行以下全部命令，全绿才能提 PR：

```powershell
# Rust 格式检查
cargo fmt --all -- --check

# Rust lint
cargo clippy --workspace --all-targets --locked -- -D warnings

# Rust 测试
cargo test --workspace --locked

# 前端构建
Set-Location apps/desktop
npm ci
npm run build
```

适配器变更额外要求：

- 运行契约测试与 Windows/macOS 黄金夹具。
- 更新 `docs/adapters/capability-matrix.md` 对应能力格。

UI 变更额外要求：

- 验证中英文本对等。
- 验证键盘导航可达。
- 验证显式失败状态（loading / error / empty）。

## 分支和 PR 流程

1. 从 `main` 创建短生命周期分支，命名 `feat/<scope>`、`fix/<scope>` 或 `docs/<scope>`。
2. 一个 PR 只做一件事，不混入无关修改。
3. 新能力先补可测验收条件；涉及领域模型、持久化或公共协议变更时新增或更新 ADR。
4. 在 `CHANGELOG.md` 的 Unreleased 下按"新增/变更/修复/安全"记录用户可感知变化。
5. 不提交 secret、Cookie、私钥、签名证书或未脱敏生产样本。
6. Commit、push、发布由维护者显式确认后执行。
7. PR 标题用祈使句，中文或英文均可，描述"做什么"而非"怎么做"。

## 新增 Adapter 流程

新增 Agent 适配器按以下步骤进行，开发命令见 ADR-0004：

1. 脚手架生成：

   ```powershell
   rigdeck adapter scaffold --output ./my-agent --id my-agent
   ```

2. 填写 `adapter.json`：声明 ID、版本、平台、检测规则、能力矩阵、原生格式、编解码、限制、官方文档链接。

3. 实现 Adapter trait 方法：`describe`、`detect`、`scan`、`validate_asset`、`render`、`plan_install`、`plan_update`、`plan_remove`、`verify`、`health`。

4. 本地验证：

   ```powershell
   rigdeck adapter validate ./my-agent
   rigdeck adapter test ./my-agent
   ```

5. 打包：

   ```powershell
   rigdeck adapter pack ./my-agent
   ```

   生成 `.rigdeck-adapter` 包，内含 manifest 和代码，附 `package_hash`。

6. 更新 `docs/adapters/capability-matrix.md`，声明该 Agent 的 Skills / Prompt / MCP 能力等级（完整 / 受限 / 人工）和保真策略。

7. 提交黄金夹具到 `crates/rigdeck-adapters/fixtures`，覆盖检测、扫描、编解码和路径安全场景。

8. 若 Adapter 需要可选 JSON-RPC helper，必须先引入显式信任决策、代码 hash 和高风险确认；不可静默执行第三方可执行载荷。

硬规则复查：Adapter 不得直接写文件；目标路径必须在检测实例声明的 surface 根目录内；不支持的能力返回 `unsupported` 或 `manual_required` 并附原因与恢复路径。
