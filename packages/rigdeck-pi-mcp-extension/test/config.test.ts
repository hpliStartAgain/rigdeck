import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { loadConfig, resolveBindings, safeToolName } from "../src/config.ts";

test("项目配置覆盖全局配置且不会自动解析明文 secret", async () => {
  const root = join(tmpdir(), `rigdeck-pi-${crypto.randomUUID()}`);
  const home = join(root, "home");
  const project = join(root, "project");
  await mkdir(join(home, ".pi", "agent"), { recursive: true });
  await mkdir(join(project, ".pi"), { recursive: true });
  await writeFile(join(home, ".pi", "agent", "mcp.json"), JSON.stringify({
    mcpServers: { demo: { command: "node", args: ["global.js"], env: { TOKEN: "${env:DEMO_TOKEN}" } } },
  }));
  await writeFile(join(project, ".pi", "mcp.json"), JSON.stringify({
    mcpServers: { demo: { url: "https://example.com/mcp", headers: { Authorization: "${env:DEMO_TOKEN}" } } },
  }));
  const loaded = await loadConfig(home, project);
  assert.equal(loaded.sources.length, 2);
  assert.equal(loaded.servers.get("demo")?.type, "streamable_http");
  assert.deepEqual(resolveBindings({ Authorization: "${env:DEMO_TOKEN}" }, { DEMO_TOKEN: "resolved" }), { Authorization: "resolved" });
});

test("拒绝 URL 凭据、远端 HTTP 和敏感明文 header", async () => {
  const root = join(tmpdir(), `rigdeck-pi-${crypto.randomUUID()}`);
  const home = join(root, "home");
  const project = join(root, "project");
  await mkdir(join(home, ".pi", "agent"), { recursive: true });
  await mkdir(project, { recursive: true });
  await writeFile(join(home, ".pi", "agent", "mcp.json"), JSON.stringify({
    mcpServers: { unsafe: { url: "http://example.com/mcp" } },
  }));
  await assert.rejects(loadConfig(home, project), /HTTPS URL/);
  assert.throws(() => resolveBindings({ Authorization: "Bearer plaintext" }, {}), /明文 secret/);
});

test("工具名确定、受限且不允许路径字符", () => {
  assert.equal(safeToolName("docs.server", "read-file"), "mcp_docs_server_read_file");
  assert.throws(() => safeToolName("..", "///"), /安全工具名/);
});
