# 配置 MCP Server 的 secret

## 目标

导入一个带凭据的 MCP Server，把凭据写入系统钥匙串，在 MCP JSON 中用 SecretRef 引用，安装后验证 Agent 配置文件不含明文。

## 前置条件

- 已安装 `rigdeck` CLI。
- 已检测到目标 Agent 实例。
- 系统钥匙串可写（Windows Credential Manager 或 macOS Keychain）。

## 步骤

### 1. 写入 MCP Server JSON

创建 `~/mcp/context.json`，凭据字段使用 `binding: secret`，`value` 填 SecretRef 标识符（不含 `=`、换行，长度 ≤ 256）：

```json
{
  "server_name": "context-mcp",
  "transport": {
    "type": "stdio",
    "command": "context-server",
    "args": ["--stdio"],
    "env": {
      "CONTEXT_API_KEY": { "binding": "secret", "value": "keychain:context-api-key" }
    }
  },
  "enabled": true,
  "timeout_ms": 30000,
  "oauth": null,
  "allowed_tools": [],
  "denied_tools": []
}
```

敏感字段若误用 `binding: literal`，`add` 会在写入库存前直接拒绝，不会留下半成品。

### 2. 把 secret 写入钥匙串

值只能通过管道或重定向配合 `--stdin` 输入，不接受明文参数：

```bash
printf '%s' 'your-actual-api-key' | rigdeck secret set keychain:context-api-key --stdin --yes
```

预期输出：

```text
已保存 SecretRef：keychain:context-api-key
```

### 3. 确认钥匙串条目存在

```bash
rigdeck secret check keychain:context-api-key
```

预期输出：

```text
SecretRef keychain:context-api-key：存在
```

### 4. 导入 MCP Server

```bash
rigdeck add ~/mcp/context.json --kind mcp --yes
```

预期输出：

```text
已导入 context-mcp（资产 asset_03...，修订 rev_03...）
```

### 5. 分配到目标 Agent

```bash
rigdeck assign asset_03... --agent <instance-id> --scope global
```

记录返回的 `计划 ID`。

### 6. 应用计划

```bash
rigdeck apply <plan-id> --plan <plan-id> --yes
```

预期输出：

```text
计划 <plan-id> 已应用，完成 N 个操作
```

## 验证

打开 Agent 写入的 MCP 配置文件（Claude Code 为 `~/.claude.json`，section `mcpServers`）：

```bash
grep -i 'api_key\|secret\|token\|bearer' ~/.claude.json
```

预期：只出现占位符或 SecretRef 标识符 `keychain:context-api-key`，不出现 `your-actual-api-key` 明文。

再验证导出包与计划也不含明文：

```bash
rigdeck export /tmp/portable.json
grep -i 'your-actual-api-key' /tmp/portable.json
```

预期：无匹配。明文凭据仅存在于钥匙串，SQLite、计划、审计、日志和导出包只保留 SecretRef。
