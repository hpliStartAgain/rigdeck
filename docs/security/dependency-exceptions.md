# 依赖漏洞审计与例外策略

## 强制门禁

RigDeck 的每次提交、拉取请求和每周定时任务都会执行以下检查：

- `cargo audit`：使用 RustSec 数据库检查根目录 `Cargo.lock`；任何已知安全漏洞都会使流水线失败。
- `npm audit --audit-level=high`：分别检查桌面前端和 Pi 扩展；高危或严重漏洞会使流水线失败。
- 审计必须显式使用 `https://registry.npmjs.org`。第三方镜像缺少安全公告接口时属于审计失败，不得解释为“没有漏洞”。
- 发布流水线使用 `cargo-cyclonedx 0.5.9` 与 npm 内置 `sbom` 命令生成四份 CycloneDX 清单，并保留 90 天。

不允许通过降低严重级别、跳过锁文件、关闭网络更新或删除审计步骤来获得绿色构建。

## 例外审批规则

安全漏洞默认不允许忽略。确实无法立即升级时，例外必须在本文件新增独立记录，并同时满足：

1. 写明公告编号、受影响版本、反向依赖链和可达性分析；
2. 指定负责人、批准人、补救计划和不超过 30 天的到期日；
3. 说明临时缓解措施，并增加能够验证该措施的自动化测试；
4. 只忽略单个公告编号，不允许使用通配符或关闭整个审计器；
5. 到期未关闭时，发布门禁自动恢复为失败。

当前没有安全漏洞例外。

## RustSec 信息性警告台账

2026-07-12 的实际审计结果没有安全漏洞，但存在 17 个信息性警告。它们不作为漏洞例外隐藏，仍会在每次 CI 日志中可见：

| 公告 | 依赖范围 | 影响判断 | 负责人 | 复核到期日 |
| --- | --- | --- | --- | --- |
| `RUSTSEC-2024-0411` 至 `RUSTSEC-2024-0420`（GTK3 共 10 项） | Tauri 在 Linux 上的 GTK3/WebKit 传递依赖 | RigDeck 0.1 发布目标仅为 Windows 与 macOS；这些 crate 不会进入对应平台的二进制。若增加 Linux 发布，必须先迁移到受维护的 GTK 绑定或重新审批。 | RigDeck 维护者 | 2026-08-11 |
| `RUSTSEC-2024-0370` | `glib-macros`、`gtk3-macros` 的构建期宏依赖 | 仅随上述 Linux GTK3 依赖进入锁文件；没有已知漏洞，风险是停止维护。 | RigDeck 维护者 | 2026-08-11 |
| `RUSTSEC-2025-0075`、`0080`、`0081`、`0098`、`0100` | `tauri-utils -> urlpattern -> unic-*` | Tauri 的解析/构建依赖；没有已知漏洞，风险是停止维护。跟随 Tauri 上游升级替换。 | RigDeck 维护者 | 2026-08-11 |
| `RUSTSEC-2024-0429` | Linux GTK3 链中的 `glib 0.18.5` | 公告涉及 `VariantStrIter` 的不健全迭代器实现；当前 Windows/macOS 发布不编译该链，项目代码也不直接调用该 API。增加 Linux 发布前必须消除此项。 | RigDeck 维护者 | 2026-08-11 |

信息性警告一旦被 RustSec 提升为漏洞，或依赖进入当前发布平台的可达路径，就立即按安全漏洞处理，不能继续沿用本表判断。

## 复核命令

```bash
cargo audit
npm audit --audit-level=high --registry=https://registry.npmjs.org --prefix apps/desktop
npm audit --audit-level=high --registry=https://registry.npmjs.org --prefix packages/rigdeck-pi-mcp-extension
```
