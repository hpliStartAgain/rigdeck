import { lstat, readFile } from "node:fs/promises";
import { isAbsolute, join, normalize, relative } from "node:path";

const MAX_CONFIG_BYTES = 1024 * 1024;
const BINDING = /^\$\{env:([A-Z_][A-Z0-9_]*)\}$/;

export interface StdioServerConfig {
  type: "stdio";
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
  timeoutMs: number;
  allowedTools: string[];
  deniedTools: string[];
}

export interface HttpServerConfig {
  type: "streamable_http";
  url: string;
  headers: Record<string, string>;
  enabled: boolean;
  timeoutMs: number;
  allowedTools: string[];
  deniedTools: string[];
}

export type ServerConfig = StdioServerConfig | HttpServerConfig;

export interface LoadedConfig {
  servers: Map<string, ServerConfig>;
  sources: string[];
}

export async function loadConfig(home: string, cwd: string): Promise<LoadedConfig> {
  const roots = [
    join(home, ".pi", "agent", "mcp.json"),
    join(cwd, ".pi", "mcp.json"),
  ];
  const servers = new Map<string, ServerConfig>();
  const sources: string[] = [];
  for (const path of roots) {
    const document = await readDocument(path);
    if (document === undefined) continue;
    sources.push(path);
    const record = asRecord(document, `${path} 根对象`);
    const entries = asRecord(record.mcpServers ?? {}, `${path}.mcpServers`);
    for (const [name, value] of Object.entries(entries)) {
      validateName(name);
      servers.set(name, parseServer(value, `${path}.mcpServers.${name}`));
    }
  }
  return { servers, sources };
}

export function resolveBindings(
  values: Record<string, string>,
  environment: NodeJS.ProcessEnv,
): Record<string, string> {
  const output: Record<string, string> = {};
  for (const [key, value] of Object.entries(values)) {
    const match = BINDING.exec(value);
    if (!match) {
      if (/authorization|token|secret|password|api[-_]?key/i.test(key)) {
        throw new Error(`${key} 必须使用 \${env:NAME}，禁止在 mcp.json 中保存明文 secret`);
      }
      output[key] = value;
      continue;
    }
    const name = match[1];
    const resolved = name ? environment[name] : undefined;
    if (!resolved) throw new Error(`环境变量 ${name ?? "?"} 不存在`);
    output[key] = resolved;
  }
  return output;
}

export function safeToolName(server: string, tool: string): string {
  const normalizedName = `mcp_${server}_${tool}`
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 96);
  if (!/^mcp_[a-z0-9_]+$/.test(normalizedName)) {
    throw new Error(`无法生成安全工具名：${server}/${tool}`);
  }
  return normalizedName;
}

function parseServer(value: unknown, location: string): ServerConfig {
  const record = asRecord(value, location);
  const enabled = optionalBoolean(record.enabled, true, `${location}.enabled`);
  const timeoutMs = optionalNumber(record.timeoutMs, 60_000, `${location}.timeoutMs`);
  if (timeoutMs < 1_000 || timeoutMs > 10 * 60_000) {
    throw new Error(`${location}.timeoutMs 必须在 1000..=600000`);
  }
  const allowedTools = stringArray(record.allowedTools, `${location}.allowedTools`);
  const deniedTools = stringArray(record.deniedTools, `${location}.deniedTools`);
  if (typeof record.command === "string") {
    validateText(record.command, `${location}.command`);
    return {
      type: "stdio",
      command: record.command,
      args: stringArray(record.args, `${location}.args`),
      env: stringRecord(record.env, `${location}.env`),
      enabled,
      timeoutMs,
      allowedTools,
      deniedTools,
    };
  }
  if (typeof record.url === "string") {
    validateUrl(record.url, location);
    return {
      type: "streamable_http",
      url: record.url,
      headers: stringRecord(record.headers, `${location}.headers`),
      enabled,
      timeoutMs,
      allowedTools,
      deniedTools,
    };
  }
  throw new Error(`${location} 必须提供 command 或 url`);
}

async function readDocument(path: string): Promise<unknown | undefined> {
  let metadata;
  try {
    metadata = await lstat(path);
  } catch (error: unknown) {
    if (isNodeError(error) && error.code === "ENOENT") return undefined;
    throw error;
  }
  if (metadata.isSymbolicLink() || !metadata.isFile()) {
    throw new Error(`MCP 配置必须是真实普通文件：${path}`);
  }
  if (metadata.size > MAX_CONFIG_BYTES) throw new Error(`MCP 配置超过 1 MiB：${path}`);
  const bytes = await readFile(path);
  return JSON.parse(bytes.toString("utf8")) as unknown;
}

function validateUrl(value: string, location: string): void {
  const url = new URL(value);
  const loopback = ["localhost", "127.0.0.1", "::1"].includes(url.hostname);
  if (url.username || url.password || (url.protocol !== "https:" && !(url.protocol === "http:" && loopback))) {
    throw new Error(`${location}.url 必须是无凭据 HTTPS URL；HTTP 仅允许 loopback`);
  }
}

function validateName(value: string): void {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(value)) throw new Error(`MCP server 名称无效：${value}`);
}

function validateText(value: string, location: string): void {
  if (!value.trim() || /[\0\r\n]/.test(value)) throw new Error(`${location} 包含非法字符`);
}

function asRecord(value: unknown, location: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error(`${location} 必须是对象`);
  return value as Record<string, unknown>;
}

function stringRecord(value: unknown, location: string): Record<string, string> {
  if (value === undefined) return {};
  const record = asRecord(value, location);
  const output: Record<string, string> = {};
  for (const [key, item] of Object.entries(record)) {
    if (typeof item !== "string") throw new Error(`${location}.${key} 必须是字符串`);
    validateText(item, `${location}.${key}`);
    output[key] = item;
  }
  return output;
}

function stringArray(value: unknown, location: string): string[] {
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) throw new Error(`${location} 必须是字符串数组`);
  return value.map((item) => {
    validateText(item as string, location);
    return item as string;
  });
}

function optionalBoolean(value: unknown, fallback: boolean, location: string): boolean {
  if (value === undefined) return fallback;
  if (typeof value !== "boolean") throw new Error(`${location} 必须是 boolean`);
  return value;
}

function optionalNumber(value: unknown, fallback: number, location: string): number {
  if (value === undefined) return fallback;
  if (typeof value !== "number" || !Number.isSafeInteger(value)) throw new Error(`${location} 必须是整数`);
  return value;
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

// 让审计工具能够确认本模块没有把配置路径规范化到根目录之外。
export function isWithin(root: string, candidate: string): boolean {
  const normalizedRoot = normalize(root);
  const normalizedCandidate = normalize(candidate);
  const child = relative(normalizedRoot, normalizedCandidate);
  return !isAbsolute(child) && child !== ".." && !child.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`);
}
