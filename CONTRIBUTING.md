# 贡献指南 / Contributing

感谢参与 RigDeck。中文是项目主要交付语言；公共协议标识符、代码符号和必要英文摘要保留英文。

## 开始之前

1. 阅读 `AGENTS.md`、`docs/adr/` 与 `docs/security/threat-model.md`。
2. 从 `main` 创建短生命周期分支；不要在提交中混入无关修改。
3. 新能力先补可测验收条件；涉及领域模型、持久化或公共协议时新增/更新 ADR。

## 架构红线

- Core 不得按 Agent 名称分支。
- Adapter 只返回投影和操作，不直接写文件。
- 任何变更必须走 Planner；不得从前端直连 SQLite 或 Agent 文件。
- 任何持久化、日志和 JSON 输出不得包含明文 secret。
- 用户可见字符串全部进入 i18n 资源。

## Rust 可读性约定

- 对关键所有权转移、借用生命周期、trait 边界和 `?` 错误传播写中文注释，解释“为什么这样设计”，避免逐行翻译语法。
- 公共 API 使用中文 rustdoc，并保留稳定英文类型/字段名。
- 优先小型值对象与穷尽 enum；禁止用字符串暗示状态机。
- `unsafe` 默认禁止；确需使用必须有 ADR、安全不变量注释和专门测试。

## 提交前验证

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

Set-Location apps/desktop
npm ci
npm run build
```

适配器变更还必须运行契约测试与 Windows/macOS 黄金夹具；UI 变更必须验证中英文本、键盘导航和显式失败状态。

## 变更说明

- 在 `CHANGELOG.md` 的 Unreleased 下按“新增/变更/修复/安全”记录用户可感知变化。
- 不提交 secret、Cookie、私钥、签名证书或未脱敏生产样本。
- Commit/push/发布由维护者显式确认后执行。

