# RigDeck Pi MCP Companion Extension

这是 RigDeck 为 Pi 提供的可选 MCP 扩展。Pi 核心刻意不内置 MCP；本包通过官方 Extension API 把用户明确配置并确认的 MCP 工具注册到当前会话。

## 安全边界

- 启动和被动浏览时绝不自动连接、启动进程或探测 server。
- 只有执行 `/rigdeck-mcp-connect` 并在交互界面确认后才连接。
- 非交互模式拒绝连接，避免 Agent 绕过人工确认。
- 远端只允许 HTTPS；HTTP 仅允许 loopback；URL 禁止 userinfo。
- 敏感 header/env 必须写成 `${env:NAME}`，配置文件不接受明文 token。
- 项目 `.pi/mcp.json` 覆盖全局 `~/.pi/agent/mcp.json` 的同名 server。
- server、工具白名单/黑名单和超时都有显式边界；会话结束时关闭全部连接。

## 配置示例

```json
{
  "mcpServers": {
    "local-docs": {
      "command": "node",
      "args": ["server.js"],
      "env": { "DOCS_TOKEN": "${env:DOCS_TOKEN}" },
      "allowedTools": ["search", "read"],
      "timeoutMs": 30000
    },
    "remote-context": {
      "url": "https://mcp.example.com/mcp",
      "headers": { "Authorization": "${env:MCP_AUTHORIZATION}" }
    }
  }
}
```

## 安装

RigDeck 桌面端/CLI 会先展示包 hash、源码路径、依赖和权限，再生成独立高风险计划。人工确认前不会写入 Pi settings，也不会启用扩展。

手工开发验证可使用 Pi 官方临时加载方式：

```bash
pi --extension ./extensions/rigdeck-mcp.ts
```

官方依据（核验日期 2026-07-11）：[Pi Extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md)、[Pi Packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md)、[MCP TypeScript SDK v1](https://github.com/modelcontextprotocol/typescript-sdk/tree/v1.x)。
