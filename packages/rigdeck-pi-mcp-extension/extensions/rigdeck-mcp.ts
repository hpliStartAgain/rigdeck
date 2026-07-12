import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

import { loadConfig, resolveBindings, safeToolName, type ServerConfig } from "../src/config.ts";

interface Connection {
  client: Client;
  toolNames: string[];
}

export default function rigdeckMcpExtension(pi: ExtensionAPI): void {
  const connections = new Map<string, Connection>();

  pi.registerCommand("rigdeck-mcp-connect", {
    description: "审查并连接 RigDeck MCP 配置（不会在启动时自动执行）",
    handler: async (_args, ctx) => {
      const loaded = await loadConfig(process.env.HOME ?? process.env.USERPROFILE ?? "", ctx.cwd);
      const enabled = [...loaded.servers.entries()].filter(([, server]) => server.enabled);
      if (enabled.length === 0) {
        ctx.ui.notify("没有已启用的 RigDeck MCP server", "info");
        return;
      }
      if (!ctx.hasUI) {
        throw new Error("非交互模式拒绝连接 MCP；请在交互界面完成显式确认");
      }
      const approved = await ctx.ui.confirm(
        "连接 RigDeck MCP server",
        `将启动/访问 ${enabled.length} 个 server：${enabled.map(([name]) => name).join(", ")}。配置来源：${loaded.sources.join(", ")}`,
      );
      if (!approved) return;
      for (const [name, server] of enabled) {
        if (connections.has(name)) continue;
        const connection = await connectServer(pi, name, server);
        connections.set(name, connection);
        ctx.ui.notify(`${name}：已注册 ${connection.toolNames.length} 个 MCP 工具`, "info");
      }
    },
  });

  pi.registerCommand("rigdeck-mcp-status", {
    description: "显示当前会话已连接的 RigDeck MCP server",
    handler: async (_args, ctx) => {
      const message = connections.size === 0
        ? "当前会话没有 MCP 连接"
        : [...connections.entries()].map(([name, value]) => `${name}: ${value.toolNames.length} tools`).join("\n");
      ctx.ui.notify(message, "info");
    },
  });

  pi.on("session_shutdown", async () => {
    await Promise.allSettled([...connections.values()].map(({ client }) => client.close()));
    connections.clear();
  });
}

async function connectServer(pi: ExtensionAPI, serverName: string, config: ServerConfig): Promise<Connection> {
  const client = new Client({ name: "rigdeck-pi-extension", version: "0.1.0" });
  const transport = config.type === "stdio"
    ? new StdioClientTransport({
        command: config.command,
        args: config.args,
        env: { ...process.env, ...resolveBindings(config.env, process.env) } as Record<string, string>,
      })
    : new StreamableHTTPClientTransport(new URL(config.url), {
        requestInit: { headers: resolveBindings(config.headers, process.env) },
      });
  await client.connect(transport);
  const listed = await client.listTools();
  const toolNames: string[] = [];
  for (const tool of listed.tools) {
    if (config.allowedTools.length > 0 && !config.allowedTools.includes(tool.name)) continue;
    if (config.deniedTools.includes(tool.name)) continue;
    const registeredName = safeToolName(serverName, tool.name);
    toolNames.push(registeredName);
    pi.registerTool({
      name: registeredName,
      label: `${serverName}: ${tool.name}`,
      description: tool.description ?? `MCP tool ${tool.name} from ${serverName}`,
      promptSnippet: `Call ${tool.name} on the explicitly connected MCP server ${serverName}`,
      promptGuidelines: [`Use ${registeredName} only when the user request requires the ${serverName} MCP server.`],
      parameters: Type.Record(Type.String(), Type.Unknown()),
      async execute(_toolCallId, params, signal) {
        if (signal?.aborted) throw new Error("MCP 工具调用已取消");
        const result = await client.callTool(
          { name: tool.name, arguments: params },
          undefined,
          { timeout: config.timeoutMs, ...(signal ? { signal } : {}) },
        );
        return {
          content: [{ type: "text", text: JSON.stringify(result.content) }],
          details: { server: serverName, tool: tool.name, isError: result.isError === true },
          isError: result.isError === true,
        };
      },
    });
  }
  return { client, toolNames };
}
