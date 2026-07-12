import { invoke } from "@tauri-apps/api/core";
import type {
  AgentInstance,
  Assignment,
  Asset,
  AssetPage,
  AssetInspection,
  AuditEvent,
  BackupManifest,
  Conflict,
  ConflictResolutionRequest,
  DeploymentPlan,
  DoctorReport,
  IpcFailure,
  RefreshOutcome,
  SearchResult,
  ServiceStatus,
  StartupRefreshStatus,
  UpdateReport,
  WatchRefreshOutcome,
} from "../types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

const demoAgent: AgentInstance = {
  id: "demo-codex-global",
  adapter_id: "codex",
  display_name: "Codex",
  version: "0.106.0",
  managed_roots: ["~/.codex"],
  health: "healthy",
  surfaces: [
    {
      asset_kind: "skill",
      scope: "global",
      path: "~/.codex/skills",
      mode: "directory_tree",
      native_format: "markdown",
    },
  ],
};

const demoAsset: Asset = {
  id: "demo-local-rust-review",
  identity: {
    source_namespace: "local",
    package: "rust-review",
    relative_path: ".",
    declared_name: "rust-review",
  },
  kind: "skill",
  current_revision_id: "demo-revision",
  state: "active",
  tags: ["rust", "review"],
};

const demoPlan: DeploymentPlan = {
  schema_version: 1,
  id: "demo-plan-preview",
  assignment_id: "demo-assignment",
  purpose: "install",
  source_hashes: ["0".repeat(64)],
  operations: [
    {
      id: "demo-operation",
      kind: "write_file",
      target_path: "~/.codex/skills/rust-review/SKILL.md",
      desired_hash: "1".repeat(64),
      rendered_diff: "+ 创建由 RigDeck 管理的 Skill 文件",
      compatibility_losses: [],
      risk: "low",
    },
  ],
  catalog_effects: [],
  risk: "low",
  created_at_ms: Date.now(),
};

function isTauri(): boolean {
  return (
    typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined
  );
}

async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isTauri()) {
    try {
      return await invoke<T>(command, args);
    } catch (error: unknown) {
      throw normalizeError(error);
    }
  }
  return demoCall<T>(command, args);
}

function normalizeError(error: unknown): IpcFailure {
  if (typeof error === "object" && error !== null) {
    const candidate = error as Record<string, unknown>;
    if (
      typeof candidate.code === "string" &&
      typeof candidate.message === "string"
    ) {
      return { code: candidate.code, message: candidate.message };
    }
  }
  return { code: "internal_error", message: String(error) };
}

async function demoCall<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  await new Promise((resolve) => window.setTimeout(resolve, 120));
  const values: Record<string, unknown> = {
    status: {
      schema_version: 1,
      agents: [demoAgent],
      asset_count: 1,
      assignment_count: 1,
      conflict_count: 0,
    } satisfies ServiceStatus,
    list_assets: [demoAsset],
    list_assets_page: {
      items: [demoAsset],
      total: 1,
      offset: Number(args?.offset ?? 0),
      limit: Number(args?.limit ?? 100),
    } satisfies AssetPage,
    list_assignments: [
      {
        id: "demo-assignment",
        asset_id: demoAsset.id,
        revision_id: "demo-revision",
        agent_instance_id: demoAgent.id,
        scope: "global",
        enabled: true,
      } satisfies Assignment,
    ],
    list_plans: [demoPlan],
    list_conflicts: [],
    list_activity: [
      {
        id: "demo-audit",
        event_type: "plan_applied",
        plan_id: demoPlan.id,
        details: { file_count: 1 },
        created_at_ms: Date.now() - 90_000,
      },
    ] satisfies AuditEvent[],
    list_backups: [],
    doctor: {
      healthy: true,
      checks: [
        { id: "database", healthy: true, message: "database 检查通过" },
        { id: "object_store", healthy: true, message: "object_store 检查通过" },
        { id: "adapters", healthy: true, message: "adapters 检查通过" },
      ],
    } satisfies DoctorReport,
    refresh: {
      adapters: [{ adapter_id: "codex", instance_count: 1 }],
      instances: [
        {
          instance: demoAgent,
          report: {
            counts: { managed_clean: 1 },
            started_at_ms: Date.now(),
            finished_at_ms: Date.now(),
          },
        },
      ],
      watch_roots: ["~/.codex"],
    } satisfies RefreshOutcome,
    poll_watcher: null,
    startup_refresh_status: {
      completed: true,
      outcome: {
        adapters: [{ adapter_id: "codex", instance_count: 1 }],
        instances: [
          {
            instance: demoAgent,
            report: {
              counts: { managed_clean: 1 },
              started_at_ms: Date.now() - 250,
              finished_at_ms: Date.now(),
            },
          },
        ],
        watch_roots: ["~/.codex"],
      },
    } satisfies StartupRefreshStatus,
    plan_assignment: demoPlan,
    plan_assignment_enabled: {
      ...demoPlan,
      id: "demo-toggle-plan",
      purpose: "remove",
      risk: "high",
    },
    apply_plan: { plan_id: demoPlan.id, files: [] },
    plan_remove_asset: [
      { ...demoPlan, id: "demo-remove", purpose: "remove", risk: "high" },
    ],
    update_assets: {
      checked: 1,
      updated: [],
      plans: [],
      skipped: [],
    } satisfies UpdateReport,
    create_backup: {
      schema_version: 1,
      id: "f".repeat(64),
      created_at_ms: Date.now(),
      database: { path: "rigdeck.sqlite3", hash: "e".repeat(64), size: 4096 },
      objects: [],
    } satisfies BackupManifest,
  };
  if (command === "inspect_asset") {
    return {
      asset: demoAsset,
      revision: {
        id: "demo-revision",
        raw_hash: "2".repeat(64),
        normalized_hash: "2".repeat(64),
        source: {
          kind: "local_folder",
          namespace: "local",
          locator: "~/skills/rust-review",
        },
        license: "MIT",
        audit: { findings: [] },
        created_at_ms: Date.now(),
      },
      files: [
        {
          path: "SKILL.md",
          hash: "2".repeat(64),
          size: 128,
          executable: false,
        },
      ],
    } as T;
  }
  if (command === "search_skills" || command === "search_mcp") {
    const isMcp = command === "search_mcp";
    return {
      items: [
        {
          id: isMcp ? "io.example/context" : "acme/rust-review",
          provider_id: isMcp ? "official-mcp-preview" : "skills.sh",
          kind: isMcp ? "mcp_server" : "skill",
          name: isMcp ? "Context Server" : "Rust Review",
          description: String(args?.query ?? ""),
          version: "1.0.0",
          license: "MIT",
          locator: isMcp
            ? "mcp-registry:io.example/context"
            : "https://skills.sh/acme/rust-review",
          namespace_verified: false,
        },
      ],
      cache_state: "network",
    } as T;
  }
  if (command === "add_skill") {
    return (await demoCall<AssetInspection>("inspect_asset")) as T;
  }
  if (command === "add_local_asset") {
    return (await demoCall<AssetInspection>("inspect_asset")) as T;
  }
  if (command === "add_mcp_registry") {
    return (await demoCall<AssetInspection>("inspect_asset")) as T;
  }
  if (command === "set_secret" || command === "delete_secret") {
    return "keychain:demo" as T;
  }
  if (command === "has_secret") {
    return false as T;
  }
  if (command === "resolve_conflict") {
    throw {
      code: "not_found",
      message: "演示数据中没有冲突",
    } satisfies IpcFailure;
  }
  const value = values[command];
  if (value === undefined) {
    throw {
      code: "unsupported",
      message: `Demo IPC: ${command}`,
    } satisfies IpcFailure;
  }
  return value as T;
}

export const api = {
  status: () => call<ServiceStatus>("status"),
  startupRefreshStatus: () =>
    call<StartupRefreshStatus>("startup_refresh_status"),
  refresh: (projectRoot?: string) =>
    call<RefreshOutcome>("refresh", { projectRoot }),
  pollWatcher: () => call<WatchRefreshOutcome | null>("poll_watcher"),
  listAssets: () => call<Asset[]>("list_assets"),
  listAssetsPage: (
    options: {
      kind?: "skill" | "prompt" | "mcp_server";
      query?: string;
      sourceNamespace?: string;
      offset?: number;
      limit?: number;
    } = {},
  ) =>
    call<AssetPage>("list_assets_page", {
      kind: options.kind,
      query: options.query,
      sourceNamespace: options.sourceNamespace,
      offset: options.offset ?? 0,
      limit: options.limit ?? 100,
    }),
  listAssignments: () => call<Assignment[]>("list_assignments"),
  inspectAsset: (assetId: string) =>
    call<AssetInspection>("inspect_asset", { assetId }),
  searchSkills: (query: string, limit = 20) =>
    call<SearchResult>("search_skills", { query, limit }),
  searchMcp: (query: string, limit = 20) =>
    call<SearchResult>("search_mcp", { query, limit }),
  addSkill: (source: string) => call<AssetInspection>("add_skill", { source }),
  addLocalAsset: (
    source: string,
    kind: "prompt" | "mcp_server",
    name?: string,
    scopes: string[] = [],
  ) => call<AssetInspection>("add_local_asset", { source, kind, name, scopes }),
  addMcpRegistry: (id: string, alias?: string) =>
    call<AssetInspection>("add_mcp_registry", { id, alias }),
  setSecret: (reference: string, value: Uint8Array) =>
    call<string>("set_secret", { reference, value: Array.from(value) }),
  hasSecret: (reference: string) => call<boolean>("has_secret", { reference }),
  deleteSecret: (reference: string) =>
    call<string>("delete_secret", { reference }),
  listPlans: (status = "pending") =>
    call<DeploymentPlan[]>("list_plans", { status }),
  planAssignment: (assetId: string, agentInstanceId: string, scope: string) =>
    call<DeploymentPlan>("plan_assignment", {
      assetId,
      agentInstanceId,
      scope,
    }),
  planAssignmentEnabled: (assignmentId: string, enabled: boolean) =>
    call<DeploymentPlan>("plan_assignment_enabled", { assignmentId, enabled }),
  applyPlan: (planId: string) =>
    call<{ plan_id: string; files: unknown[] }>("apply_plan", { planId }),
  updateAssets: () => call<UpdateReport>("update_assets"),
  planRemoveAsset: (assetId: string) =>
    call<DeploymentPlan[]>("plan_remove_asset", { assetId }),
  listConflicts: () => call<Conflict[]>("list_conflicts"),
  showConflict: (conflictId: string) =>
    call<Conflict>("show_conflict", { conflictId }),
  resolveConflict: (conflictId: string, request: ConflictResolutionRequest) =>
    call<DeploymentPlan>("resolve_conflict", { conflictId, request }),
  listActivity: (limit = 100) => call<AuditEvent[]>("list_activity", { limit }),
  doctor: () => call<DoctorReport>("doctor"),
  createBackup: () => call<BackupManifest>("create_backup"),
  listBackups: () => call<BackupManifest[]>("list_backups"),
  restoreBackup: (backupId: string) =>
    call<BackupManifest>("restore_backup", { backupId }),
};

export { normalizeError };
