# 性能预算与复核方法

## 参考机器

GA 的正式基准机器固定为：Intel Core i5-12400、16 GB RAM、NVMe SSD，Windows 11 x64。发布报告必须记录操作系统版本、磁盘型号、Rust/Node 版本、提交 hash、20 次样本和 P95；更快的开发机或 GitHub 托管 Runner 只能提供回归证据，不能替代正式参考机器报告。

## 自动门禁

CI 的“有界性能回归”作业每次运行 20 个样本并打印 P95：

| 路径 | 夹具 | 预算 |
| --- | --- | --- |
| SQLite 本地库存分页 | 10,000 个资产、每页 50 条 | P95 < 250 ms |
| Adapter 冷扫描 | 1,000 个典型单文件 Skill | P95 < 3 s |
| 文件 watcher | 普通本地文件创建/突发改写 | 2 s 内可见且去重 |
| 桌面首次可交互 | 新导航到总览标题可见 | < 1.5 s；扫描在后台继续 |

桌面启动只读取最多 200 个资产供通用选择器使用；Library 每页 50 条，装配搜索最多 100 条。总数使用 `COUNT(*)`，筛选和分页在 SQLite 内执行，因此 React 状态和 DOM 大小不随完整库存线性增长。

## 本地复核

```bash
cargo test -p rigdeck-store ten_thousand_assets_are_counted_and_paged_inside_sqlite --locked -- --nocapture
cargo test -p rigdeck-adapters cold_scan_of_one_thousand_typical_skills_meets_budget --locked -- --nocapture
cargo test -p rigdeck-adapters watcher_reports_local_change_within_budget --locked -- --nocapture
```

桌面计时从新窗口导航开始，到一级标题“当前装配状态”可见且可获得焦点为止。必须同时检查 390×844 窄屏无横向溢出，并在 125%、150%、200% Windows 缩放及 macOS Retina 上完成发布矩阵。
