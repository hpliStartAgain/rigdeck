export type AssetKind = "skill" | "prompt" | "mcp_server";
export type RiskLevel = "low" | "medium" | "high" | "critical";

export interface NativeSurface {
  asset_kind: AssetKind;
  scope: string;
  path: string;
  mode: string;
  native_format: string;
}

export interface AgentInstance {
  id: string;
  adapter_id: string;
  display_name: string;
  version?: string;
  managed_roots: string[];
  profile?: string;
  health: string;
  surfaces: NativeSurface[];
}

export interface Assignment {
  id: string;
  asset_id: string;
  revision_id: string;
  agent_instance_id: string;
  scope: string;
  enabled: boolean;
}

export interface AssetIdentity {
  source_namespace: string;
  package: string;
  relative_path: string;
  declared_name: string;
}

export interface Asset {
  id: string;
  identity: AssetIdentity;
  kind: AssetKind;
  current_revision_id?: string;
  state: string;
  tags: string[];
}

export interface AssetPage {
  items: Asset[];
  total: number;
  offset: number;
  limit: number;
}

export interface AuditFinding {
  code: string;
  severity: string;
  message: string;
  path?: string;
}

export interface AssetRevision {
  id: string;
  raw_hash: string;
  normalized_hash: string;
  source: {
    kind: string;
    namespace: string;
    locator: string;
    revision?: string;
  };
  license?: string;
  audit: { findings: AuditFinding[] };
  created_at_ms: number;
  author?: string;
  update_time_ms?: number;
  platform_restrictions?: string[];
}

export interface AssetInspection {
  asset: Asset;
  revision: AssetRevision;
  files: Array<{
    path: string;
    hash: string;
    size: number;
    executable: boolean;
  }>;
}

export interface PlannedOperation {
  id: string;
  kind: "write_file" | "remove_file" | "adopt_baseline";
  target_path: string;
  expected_target_hash?: string;
  desired_hash?: string;
  rollback_object?: string;
  rendered_diff: string;
  compatibility_losses: Array<{
    code: string;
    message: string;
    blocking: boolean;
  }>;
  risk: RiskLevel;
}

export interface DeploymentPlan {
  schema_version: number;
  id: string;
  assignment_id?: string;
  purpose: "install" | "update" | "remove" | "restore" | "conflict_resolution";
  source_hashes: string[];
  operations: PlannedOperation[];
  catalog_effects: Array<{
    kind: string;
    asset_id?: string;
    assignment_id?: string;
    revision_id?: string;
    plan_id?: string;
  }>;
  risk: RiskLevel;
  created_at_ms: number;
}

export interface Conflict {
  id: string;
  kind: string;
  cause: string;
  affected: string[];
  risk: string;
  actions: ResolutionAction[];
  resolved: boolean;
  agent_instance_id?: string;
  assignment_id?: string;
  baseline_hash?: string;
  current_hash?: string;
  selected_action?: ResolutionAction;
  resolution_plan_id?: string;
  resolved_at_ms?: number;
}

export type ResolutionAction =
  | "keep_rigdeck_revision"
  | "import_agent_revision"
  | "keep_agent_fork"
  | "rename_and_coexist"
  | "three_way_merge"
  | "per_file_selection"
  | "abandon_plan"
  | "restore_backup";

export type FileResolutionChoice = "keep_rigdeck" | "keep_agent" | "merged";

export interface FileResolution {
  path: string;
  choice: FileResolutionChoice;
  merged_content?: string;
}

export interface ConflictResolutionRequest {
  action: ResolutionAction;
  rename_to?: string;
  backup_id?: string;
  merged_content?: string;
  files: FileResolution[];
}

export interface AuditEvent {
  id: string;
  event_type: string;
  plan_id?: string;
  details: unknown;
  created_at_ms: number;
}

export interface BackupManifest {
  schema_version: number;
  id: string;
  created_at_ms: number;
  database: { path: string; hash: string; size: number };
  objects: Array<{ path: string; hash: string; size: number }>;
}

export interface ServiceStatus {
  schema_version: number;
  agents: AgentInstance[];
  asset_count: number;
  assignment_count: number;
  conflict_count: number;
}

export interface DoctorReport {
  healthy: boolean;
  checks: Array<{ id: string; healthy: boolean; message: string }>;
}

export interface VulnerabilityInfo {
  id: string;
  severity: string;
  affected_versions?: string;
  fixed_in?: string;
  url?: string;
}

export interface CatalogItem {
  id: string;
  provider_id: string;
  kind: AssetKind;
  name: string;
  description?: string;
  version?: string;
  license?: string;
  locator: string;
  namespace_verified: boolean;
  vulnerabilities?: VulnerabilityInfo[];
}

export interface SearchResult {
  items: CatalogItem[];
  cache_state: "network" | "revalidated" | "stale_offline";
  rate_limit?: { limit?: number; remaining?: number; reset?: number };
}

export interface RefreshOutcome {
  adapters: Array<{
    adapter_id: string;
    instance_count: number;
    error?: string;
  }>;
  instances: Array<{
    instance: AgentInstance;
    report: {
      counts: Record<string, number>;
      started_at_ms: number;
      finished_at_ms: number;
    };
  }>;
  watch_roots: string[];
}

export interface StartupRefreshStatus {
  completed: boolean;
  outcome?: RefreshOutcome;
  error?: string;
}

export interface WatchRefreshOutcome {
  changed_paths: string[];
  kinds: string[];
  refresh: RefreshOutcome;
}

export interface UpdateReport {
  checked: number;
  updated: AssetInspection[];
  plans: DeploymentPlan[];
  skipped: string[];
}

export interface IpcFailure {
  code: string;
  message: string;
}
