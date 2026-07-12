# 故障排查

按问题分类列出常见症状、原因和解决方法。命令以 `rigdeck` CLI 为主，数据目录默认位置：

- Windows：`%LOCALAPPDATA%\RigDeck`
- macOS：`~/Library/Application Support/RigDeck`
- Linux：`$XDG_DATA_HOME/rigdeck` 或 `~/.local/share/rigdeck`

## 启动问题

### Agent 检测不到

- 症状：启动后 Agent 列表为空，或某个已安装的 Agent 不出现。
- 原因：Agent 未安装到默认路径；Adapter 检测规则未覆盖当前安装位置；平台不受支持。
- 解决方法：
  1. 运行 `rigdeck doctor --json`，查看 `agent_paths` 和 `adapters` 检查项。
  2. 确认 Agent 配置目录存在且为真实目录（非 symlink）。
  3. 用 `rigdeck agents --json` 查看检测到的实例和版本。
  4. 若路径非默认，手动创建软链或设置环境变量指向真实安装路径。

### 版本显示 null

- 症状：Agent 实例存在，但 version 字段为 `null`。
- 原因：Adapter 版本提取规则未匹配到当前 Agent 的版本标记文件或命令输出格式。
- 解决方法：
  1. 查 `docs/adapters/capability-matrix.md` 确认该 Agent 的官方路径和版本来源。
  2. 手动运行该 Agent 的版本命令，确认输出格式。
  3. 若格式变化，提交 Adapter 修复并附带黄金夹具。

## 导入问题

### 路径无效

- 症状：导入报错 `path_violation` 或 `资产 relative_path 必须是不能向上逃逸的相对路径`。
- 原因：资产路径包含 `..`、绝对路径或路径分隔符出现在 `declared_name` 中。
- 解决方法：
  1. 检查来源包结构，确认相对路径不向上逃逸。
  2. `declared_name` 只用文件名，不含 `/` 或 `\`。
  3. 用 `rigdeck adapter validate <path>` 验证包结构。

### 归档损坏

- 症状：解压报错 `ValidationFailed` 或 `Io`，解压中断。
- 原因：归档不完整、超出发炸弹限制（最大文件数/大小）、或包含路径穿越条目。
- 解决方法：
  1. 重新下载归档，校验大小和哈希。
  2. 本地用 `tar -tzf` 或 `unzip -l` 预检条目数和路径。
  3. 若条目包含绝对路径或 `..`，归档被安全策略拒绝，需来源方修正。

### symlink 拒绝

- 症状：导入或应用报错 `Agent 管理根必须是真实目录且不能是 symlink`。
- 原因：Agent 管理根路径是符号链接，RigDeck 拒绝在 symlink 上写入。
- 解决方法：
  1. 运行 `rigdeck doctor --json`，查看 `agent_paths` 检查项定位具体路径。
  2. 将 symlink 替换为真实目录，或把内容移到真实路径后重新指向。
  3. 重新运行 `rigdeck agents --json` 确认路径更新。

## 计划问题

### 计划过期

- 症状：应用计划报错 `PlanInvalidated`，提示源或目标哈希已变化。
- 原因：生成计划后到应用前，源资产或目标文件被外部修改；或 pending 计划超过 `STALE_PLAN_AFTER_MS`。
- 解决方法：
  1. 重新生成计划：`rigdeck plan <assignment-id>`。
  2. 确认无外部进程同时修改目标文件。
  3. `rigdeck doctor --json` 中 `stale_plans` 检查项会列出所有过期 pending 计划。

### hash 不匹配

- 症状：应用后验证阶段报错，目标文件实际哈希与计划预期不符。
- 原因：写入过程中文件被外部修改；或文件系统编码/换行符被自动转换。
- 解决方法：
  1. 检查是否有编辑器、同步工具或 Agent 自身在写入后修改文件。
  2. 关闭对该路径的外部监听，重新生成并应用计划。
  3. 若是换行符问题，确认 git 未配置 autocrlf 强制转换。

## 冲突问题

### 解决后仍显示未解决

- 症状：选择了解决动作，但刷新后冲突仍标记为未解决。
- 原因：解决动作是"放弃计划"，磁盘冲突保持原状；或关联计划尚未成功提交；或存在多个 pending 计划，旧计划未变为 `abandoned`。
- 解决方法：
  1. 用 `rigdeck conflicts --json` 查看冲突状态和关联计划。
  2. 确认选择的动作不是 `AbandonPlan`（该动作故意留下未解决冲突）。
  3. 重新选择其他动作并确保计划应用成功。
  4. 若旧 pending 计划残留，重新提交解决会自动将旧计划标记为 `abandoned`。

### 三方合并正文丢失

- 症状：提交合并正文后报错，找不到正文内容。
- 原因：合并正文存入加密对象库，冲突记录和计划只保留对象 hash；若对象库损坏则无法回溯。
- 解决方法：
  1. `rigdeck doctor --json` 检查 `object_store` 完整性。
  2. 从备份恢复对象库：`rigdeck restore <backup-id> --yes`。
  3. 重新提交合并正文。

## 备份问题

### 恢复失败

- 症状：`rigdeck restore` 报错，提示 manifest 或密文对象验证失败。
- 原因：备份对象库损坏、密钥丢失、或备份 ID 不存在。
- 解决方法：
  1. `rigdeck doctor --json` 查看备份检查项和对象库完整性。
  2. 确认钥匙串中对象库密钥存在（`keychain` 检查项）。
  3. 尝试其他备份 ID：`rigdeck backup list --json`（若可用）。
  4. 恢复只读打开备份数据库生成恢复计划，不直接替换当前数据库；确认计划后应用。

### 对象库损坏

- 症状：`object_store` 完整性检查失败，对象缺失或哈希不符。
- 原因：磁盘错误、强制关机、外部工具删除对象文件。
- 解决方法：
  1. 优先从最近健康备份恢复。
  2. 若无可用备份，可从 Agent 当前原生状态重建清单：RigDeck 支持缓存/索引丢失后从 Agent 端重建 inventory。
  3. 不可逆损坏时，将数据目录重命名备份后重新初始化，重新导入资产。

## 权限问题

### 钥匙串访问被拒

- 症状：`keychain` 检查项失败，提示无法读取对象库密钥；或 MCP 凭据读取报错。
- 原因：操作系统钥匙串拒绝访问、密钥条目被删除、或应用签名/权限变更导致钥匙串授权失效。
- 解决方法：
  1. Windows：打开"凭据管理器"，确认 `rigdeck-object-store-v1` 条目存在。
  2. macOS：打开"钥匙串访问"，确认 `rigdeck-object-store-v1` 条目且应用有访问权限。
  3. 条目缺失时，删除数据目录后重新初始化（会生成新密钥，旧加密对象需从备份恢复）。
  4. `rigdeck doctor --json` 中 `keychain` 检查项验证密钥长度是否为 32 字节。

### 目录无写权限

- 症状：应用计划报错 `Io`，或启动时报数据库/对象库无法打开。
- 原因：数据目录或 Agent 管理根目录当前用户无写权限。
- 解决方法：
  1. 确认数据目录权限：Windows 用 `icacls`，macOS/Linux 用 `ls -ld`。
  2. 授予当前用户读写权限。
  3. 或用 `--data-dir` 指定有权限的目录：`rigdeck --data-dir <path> doctor`。
  4. Agent 管理根权限不足时，调整对应目录权限后重新运行 `rigdeck doctor --json`。
