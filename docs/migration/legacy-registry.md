# 旧 Registry 一次性迁移规范

旧 `agent-skill-registry` 只能作为迁移输入与反例资料，RigDeck 不读取其固定路径、不复用其 manifest 作为事实源，也不依赖其脚本运行。

## 可导入内容

- 已知 Skill 目录及其声明名称、相对路径和可推断来源。
- Agent 安装视图，用于生成“现状候选”，不能直接标记为 RigDeck 已托管。
- 全局 instruction 文件中的用户内容，只有经用户选择后才转换为 Prompt 资产。

## 映射规则

1. 为每个候选计算 raw/normalized hash，扫描文件清单与风险。
2. 能证明来源时使用 `namespace + repository/package + relative path + declared name`；不能证明时标记 `local-import` provenance。
3. 相同目录名不自动合并；内容相同可去重对象，但保留不同 provenance。
4. 导入只创建资产与候选分配，不直接覆盖 Agent 文件。
5. 任何明文凭据都替换为待绑定 `SecretRef`，原值不进入数据库或导出。

## 禁止形成持续依赖

- 不监控旧 Registry 清单。
- 不从旧脚本执行 install/sync。
- 不把旧目录作为 RigDeck 对象库。
- 导入完成后删除旧目录不是 RigDeck 的自动操作；由用户自行决定。

