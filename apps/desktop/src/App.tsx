import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import {
  Activity,
  AlertTriangle,
  ArchiveRestore,
  Bot,
  Boxes,
  Check,
  ChevronRight,
  CircleHelp,
  Command,
  DatabaseBackup,
  FileCode2,
  Globe2,
  Languages,
  LayoutDashboard,
  Library,
  LoaderCircle,
  Moon,
  PackagePlus,
  Play,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  Sun,
  TriangleAlert,
  Wrench,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";

import { api, normalizeError } from "./lib/api";
import type {
  AgentInstance,
  Assignment,
  Asset,
  AssetInspection,
  AuditEvent,
  BackupManifest,
  CatalogItem,
  Conflict,
  ConflictResolutionRequest,
  DeploymentPlan,
  DoctorReport,
  IpcFailure,
  ResolutionAction,
  FileResolutionChoice,
  RiskLevel,
  ServiceStatus,
  StartupRefreshStatus,
} from "./types";

type Page =
  | "overview"
  | "library"
  | "agents"
  | "assemble"
  | "conflicts"
  | "activity"
  | "settings";
type Theme = "system" | "porcelain" | "obsidian" | "aurora";

const navItems: Array<{ id: Page; icon: typeof LayoutDashboard }> = [
  { id: "overview", icon: LayoutDashboard },
  { id: "library", icon: Library },
  { id: "agents", icon: Bot },
  { id: "assemble", icon: Command },
  { id: "conflicts", icon: TriangleAlert },
  { id: "activity", icon: Activity },
  { id: "settings", icon: Settings },
];

export default function App() {
  const { t } = useTranslation();
  const [page, setPage] = useState<Page>("overview");
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [assets, setAssets] = useState<Asset[]>([]);
  const [assignments, setAssignments] = useState<Assignment[]>([]);
  const [conflicts, setConflicts] = useState<Conflict[]>([]);
  const [activity, setActivity] = useState<AuditEvent[]>([]);
  const [backups, setBackups] = useState<BackupManifest[]>([]);
  const [pendingPlans, setPendingPlans] = useState<DeploymentPlan[]>([]);
  const [startupRefresh, setStartupRefresh] = useState<StartupRefreshStatus>({
    completed: false,
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<IpcFailure | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    const stored = localStorage.getItem("rigdeck-theme");
    const theme: Theme =
      stored === "porcelain" || stored === "obsidian" || stored === "aurora"
        ? stored
        : "system";
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const sync = () => applyTheme(theme);
    sync();
    if (theme === "system") media.addEventListener("change", sync);
    return () => media.removeEventListener("change", sync);
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [
        nextStatus,
        nextAssets,
        nextAssignments,
        nextConflicts,
        nextActivity,
        nextBackups,
        nextPlans,
      ] = await Promise.all([
        api.status(),
        // 启动只取有界首屏，10,000 条库存不会一次性进入 React 状态树。
        api.listAssetsPage({ limit: 200 }),
        api.listAssignments(),
        api.listConflicts(),
        api.listActivity(),
        api.listBackups(),
        api.listPlans(),
      ]);
      setStatus(nextStatus);
      setAssets(nextAssets.items);
      setAssignments(nextAssignments);
      setConflicts(nextConflicts);
      setActivity(nextActivity);
      setBackups(nextBackups);
      setPendingPlans(nextPlans);
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    if (!startupRefresh.completed || startupRefresh.error) return;
    let cancelled = false;
    let timer: number | undefined;
    const poll = async () => {
      try {
        const changed = await api.pollWatcher();
        if (cancelled) return;
        if (changed) await reload();
      } catch (caught: unknown) {
        if (!cancelled) setError(normalizeError(caught));
        return;
      }
      timer = window.setTimeout(() => void poll(), 500);
    };
    timer = window.setTimeout(() => void poll(), 500);
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [reload, startupRefresh.completed, startupRefresh.error]);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    const poll = async () => {
      try {
        const result = await api.startupRefreshStatus();
        if (cancelled) return;
        setStartupRefresh(result);
        if (result.completed) {
          if (result.error)
            setError({ code: "startup_refresh_failed", message: result.error });
          await reload();
          return;
        }
      } catch (caught: unknown) {
        if (!cancelled) setError(normalizeError(caught));
        return;
      }
      timer = window.setTimeout(() => void poll(), 250);
    };
    void poll();
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [reload]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 4200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const agents = status?.agents ?? [];
  const common = { assets, agents, setError, setToast, reload };

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label={t("common.navigation")}>
        <div className="brand-block">
          <img src="/favicon.png" alt="" className="brand-mark" />
          <div>
            <div className="brand-name">RigDeck</div>
            <div className="brand-tagline">{t("brand.tagline")}</div>
          </div>
        </div>
        <nav className="nav-list">
          {navItems.map(({ id, icon: Icon }) => (
            <button
              key={id}
              type="button"
              className="nav-item"
              data-active={page === id}
              aria-current={page === id ? "page" : undefined}
              aria-label={t(`nav.${id}`)}
              onClick={() => setPage(id)}
            >
              <Icon aria-hidden="true" size={18} />
              <span>{t(`nav.${id}`)}</span>
              {id === "conflicts" && conflicts.length > 0 ? (
                <span className="nav-badge">{conflicts.length}</span>
              ) : null}
            </button>
          ))}
        </nav>
        {window.__TAURI_INTERNALS__ === undefined ? (
          <div className="preview-pill">
            <Globe2 size={14} />
            {t("common.previewMode")}
          </div>
        ) : null}
      </aside>

      <main className="main-content" id="main-content">
        {loading && !status ? <LoadingState /> : null}
        {error && !status ? (
          <ErrorState error={error} onRetry={() => void reload()} />
        ) : null}
        {status ? (
          <>
            {page === "overview" ? (
              <OverviewPage
                status={status}
                conflicts={conflicts}
                plans={pendingPlans}
                startupRefresh={startupRefresh}
                {...common}
              />
            ) : null}
            {page === "library" ? <LibraryPage {...common} /> : null}
            {page === "agents" ? (
              <AgentsPage
                agents={agents}
                assets={assets}
                assignments={assignments}
                setError={setError}
                setToast={setToast}
                reload={reload}
              />
            ) : null}
            {page === "assemble" ? <AssemblePage {...common} /> : null}
            {page === "conflicts" ? (
              <ConflictsPage conflicts={conflicts} {...common} />
            ) : null}
            {page === "activity" ? (
              <ActivityPage activity={activity} backups={backups} {...common} />
            ) : null}
            {page === "settings" ? <SettingsPage setError={setError} /> : null}
          </>
        ) : null}
      </main>

      {toast ? (
        <div className="toast" role="status">
          <Check size={17} />
          {toast}
        </div>
      ) : null}
      {error && status ? (
        <div className="error-banner" role="alert">
          <AlertTriangle size={18} />
          <span>{error.message}</span>
          <button
            type="button"
            onClick={() => setError(null)}
            aria-label={t("common.close")}
          >
            <X size={16} />
          </button>
        </div>
      ) : null}
    </div>
  );
}

interface CommonPageProps {
  assets: Asset[];
  agents: AgentInstance[];
  setError: (error: IpcFailure | null) => void;
  setToast: (message: string | null) => void;
  reload: () => Promise<void>;
}

function PageHeader({
  title,
  subtitle,
  action,
}: {
  title: string;
  subtitle: string;
  action?: ReactNode;
}) {
  return (
    <header className="page-header">
      <div>
        <h1>{title}</h1>
        <p>{subtitle}</p>
      </div>
      {action}
    </header>
  );
}

function OverviewPage({
  status,
  conflicts,
  plans,
  startupRefresh,
  setError,
  setToast,
  reload,
}: CommonPageProps & {
  status: ServiceStatus;
  conflicts: Conflict[];
  plans: DeploymentPlan[];
  startupRefresh: StartupRefreshStatus;
}) {
  const { t } = useTranslation();
  const [refreshing, setRefreshing] = useState(false);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const runRefresh = async () => {
    setRefreshing(true);
    try {
      const result = await api.refresh();
      setToast(
        t("overview.refreshDone", {
          instances: result.instances.length,
          roots: result.watch_roots.length,
        }),
      );
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setRefreshing(false);
    }
  };
  const checkUpdates = async () => {
    setCheckingUpdates(true);
    try {
      const report = await api.updateAssets();
      setToast(
        t("overview.updateDone", {
          updated: report.updated.length,
          plans: report.plans.length,
        }),
      );
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setCheckingUpdates(false);
    }
  };
  const cards = [
    ["overview.agents", status.agents.length, Bot],
    ["overview.assets", status.asset_count, Boxes],
    ["overview.assignments", status.assignment_count, Sparkles],
    ["overview.conflicts", status.conflict_count, TriangleAlert],
  ] as const;
  const startupInstances = startupRefresh.outcome?.instances.length ?? 0;
  const startupRoots = startupRefresh.outcome?.watch_roots.length ?? 0;
  return (
    <section>
      <PageHeader
        title={t("overview.title")}
        subtitle={t("overview.subtitle")}
        action={
          <div className="header-actions">
            <Button
              variant="ghost"
              loading={checkingUpdates}
              onClick={() => void checkUpdates()}
            >
              {t("overview.checkUpdates")}
            </Button>
            <Button
              icon={RefreshCw}
              loading={refreshing}
              onClick={() => void runRefresh()}
            >
              {t("common.refresh")}
            </Button>
          </div>
        }
      />
      <div
        className="startup-summary"
        data-completed={startupRefresh.completed}
      >
        <RefreshCw size={16} />
        <span>
          {startupRefresh.completed
            ? startupRefresh.error
              ? t("overview.startupFailed")
              : t("overview.startupDone", {
                  instances: startupInstances,
                  roots: startupRoots,
                })
            : t("overview.startupScanning")}
        </span>
      </div>
      <div className="metric-grid">
        {cards.map(([label, value, Icon]) => (
          <article className="metric-card" key={label}>
            <div className="metric-icon">
              <Icon size={20} />
            </div>
            <div>
              <span>{t(label)}</span>
              <strong>{value}</strong>
            </div>
          </article>
        ))}
      </div>
      <div className="content-grid two-columns">
        <Panel
          title={t("overview.healthTitle")}
          icon={<ShieldCheck size={18} />}
        >
          {status.agents.length === 0 ? (
            <EmptyState />
          ) : (
            <div className="stack-list">
              {status.agents.map((agent) => (
                <div className="agent-row" key={agent.id}>
                  <span className="agent-avatar">
                    {agent.display_name.slice(0, 1)}
                  </span>
                  <div>
                    <strong>{agent.display_name}</strong>
                    <small>{agent.profile ?? agent.adapter_id}</small>
                  </div>
                  <StatusBadge healthy={agent.health === "healthy"} />
                </div>
              ))}
            </div>
          )}
        </Panel>
        <Panel title={t("overview.notices")} icon={<CircleHelp size={18} />}>
          {conflicts.length === 0 && plans.length === 0 ? (
            <div className="clear-state">
              <Check size={24} />
              <p>{t("overview.allClear")}</p>
            </div>
          ) : (
            <div className="stack-list">
              {plans
                .filter((plan) => plan.purpose === "update")
                .slice(0, 3)
                .map((plan) => (
                  <div className="notice-row" key={plan.id}>
                    <RefreshCw size={18} />
                    <div>
                      <strong>{t("overview.availableUpdate")}</strong>
                      <small>
                        {t("overview.planSummary", {
                          operations: plan.operations.length,
                          risk: t(`risk.${plan.risk}`),
                        })}
                      </small>
                    </div>
                  </div>
                ))}
              {plans
                .filter(
                  (plan) => plan.risk === "high" || plan.risk === "critical",
                )
                .slice(0, 3)
                .map((plan) => (
                  <div className="notice-row" key={`risk:${plan.id}`}>
                    <AlertTriangle size={18} />
                    <div>
                      <strong>{t("overview.pendingRisk")}</strong>
                      <small>{plan.id.slice(0, 16)}</small>
                    </div>
                  </div>
                ))}
              {conflicts.slice(0, 5).map((conflict) => (
                <div className="notice-row" key={conflict.id}>
                  <TriangleAlert size={18} />
                  <div>
                    <strong>{localizedLabel(t, conflict.kind)}</strong>
                    <small>{conflict.cause}</small>
                  </div>
                </div>
              ))}
            </div>
          )}
        </Panel>
      </div>
    </section>
  );
}

function LibraryPage({
  assets,
  agents,
  setError,
  setToast,
  reload,
}: CommonPageProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"skill" | "prompt" | "mcp">("skill");
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("");
  const [results, setResults] = useState<CatalogItem[]>([]);
  const [searching, setSearching] = useState(false);
  const [importing, setImporting] = useState(false);
  const [cacheState, setCacheState] = useState<string | null>(null);
  const [sourceFilter, setSourceFilter] = useState("");
  const [localQuery, setLocalQuery] = useState("");
  const [installedAssets, setInstalledAssets] = useState<Asset[]>(assets);
  const [installedTotal, setInstalledTotal] = useState(assets.length);
  const [installedOffset, setInstalledOffset] = useState(0);
  const [installedLoading, setInstalledLoading] = useState(false);
  const [inspection, setInspection] = useState<AssetInspection | null>(null);
  const installedLimit = 50;
  const filtered = installedAssets;
  const compatibleAgents = inspection
    ? agents
        .filter((agent) =>
          agent.surfaces.some(
            (surface) => surface.asset_kind === inspection.asset.kind,
          ),
        )
        .map((agent) => agent.display_name)
    : [];

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setInstalledLoading(true);
      void api
        .listAssetsPage({
          kind: tab === "mcp" ? "mcp_server" : tab,
          query: localQuery,
          sourceNamespace: sourceFilter,
          offset: installedOffset,
          limit: installedLimit,
        })
        .then((page) => {
          if (cancelled) return;
          setInstalledAssets(page.items);
          setInstalledTotal(page.total);
        })
        .catch((caught: unknown) => {
          if (!cancelled) setError(normalizeError(caught));
        })
        .finally(() => {
          if (!cancelled) setInstalledLoading(false);
        });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [assets, installedOffset, localQuery, setError, sourceFilter, tab]);

  const searchCatalog = async (event: FormEvent) => {
    event.preventDefault();
    if (query.trim().length < 2) return;
    setSearching(true);
    try {
      if (tab === "prompt") return;
      const response =
        tab === "mcp"
          ? await api.searchMcp(query)
          : await api.searchSkills(query);
      setResults(response.items);
      setCacheState(response.cache_state);
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setSearching(false);
    }
  };
  const importAsset = async (event: FormEvent) => {
    event.preventDefault();
    if (!source.trim()) return;
    setImporting(true);
    try {
      const result =
        tab === "skill"
          ? await api.addSkill(source.trim())
          : await api.addLocalAsset(
              source.trim(),
              tab === "prompt" ? "prompt" : "mcp_server",
            );
      setToast(
        t("library.imported", { name: result.asset.identity.declared_name }),
      );
      setSource("");
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setImporting(false);
    }
  };
  const inspect = async (asset: Asset) => {
    try {
      setInspection(await api.inspectAsset(asset.id));
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    }
  };
  const importCatalog = async (item: CatalogItem) => {
    setImporting(true);
    try {
      const result =
        tab === "mcp"
          ? await api.addMcpRegistry(item.id)
          : await api.addSkill(`skills.sh:${item.id}`);
      setToast(
        t("library.imported", { name: result.asset.identity.declared_name }),
      );
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setImporting(false);
    }
  };
  return (
    <section>
      <PageHeader title={t("library.title")} subtitle={t("library.subtitle")} />
      <div className="segmented" role="tablist">
        {(["skill", "prompt", "mcp"] as const).map((value) => (
          <button
            type="button"
            role="tab"
            aria-selected={tab === value}
            data-active={tab === value}
            key={value}
            onClick={() => {
              setTab(value);
              setInstalledOffset(0);
            }}
          >
            {t(`library.${value}`)}
          </button>
        ))}
      </div>
      <div className="content-grid two-columns library-grid">
        <Panel title={t("library.installed")} icon={<Library size={18} />}>
          <div className="form-grid library-filters">
            <Field label={t("library.localSearch")}>
              <input
                value={localQuery}
                onChange={(event) => {
                  setLocalQuery(event.target.value);
                  setInstalledOffset(0);
                }}
                placeholder={t("library.localSearchPlaceholder")}
              />
            </Field>
            <Field label={t("library.sourceFilter")}>
              <input
                value={sourceFilter}
                onChange={(event) => {
                  setSourceFilter(event.target.value);
                  setInstalledOffset(0);
                }}
                placeholder={t("library.allSources")}
              />
            </Field>
          </div>
          {installedLoading ? (
            <div className="loading-inline">
              <LoaderCircle className="spin" size={18} />
              {t("common.loading")}
            </div>
          ) : filtered.length === 0 ? (
            <EmptyState />
          ) : (
            <div className="asset-grid">
              {filtered.map((asset) => (
                <button
                  type="button"
                  className="asset-card"
                  key={asset.id}
                  onClick={() => void inspect(asset)}
                >
                  <div className="asset-card-title">
                    <FileCode2 size={18} />
                    <strong>{asset.identity.declared_name}</strong>
                  </div>
                  <p>{asset.identity.package}</p>
                  <div className="chip-row">
                    <span className="chip">
                      {asset.identity.source_namespace}
                    </span>
                    {asset.tags.map((tag) => (
                      <span className="chip muted" key={tag}>
                        {tag}
                      </span>
                    ))}
                  </div>
                </button>
              ))}
            </div>
          )}
          <div className="pagination">
            <Button
              variant="ghost"
              disabled={installedOffset === 0 || installedLoading}
              onClick={() =>
                setInstalledOffset(
                  Math.max(0, installedOffset - installedLimit),
                )
              }
            >
              {t("common.previous")}
            </Button>
            <span>
              {t("library.pageSummary", {
                start: installedTotal === 0 ? 0 : installedOffset + 1,
                end: Math.min(installedOffset + installedLimit, installedTotal),
                total: installedTotal,
              })}
            </span>
            <Button
              variant="ghost"
              disabled={
                installedOffset + installedLimit >= installedTotal ||
                installedLoading
              }
              onClick={() =>
                setInstalledOffset(installedOffset + installedLimit)
              }
            >
              {t("common.next")}
            </Button>
          </div>
          {inspection ? (
            <div className="inspection-box">
              <h3>{inspection.asset.identity.declared_name}</h3>
              <InfoRow
                label={t("library.source")}
                value={inspection.revision.source.locator}
                mono
              />
              <InfoRow
                label={t("library.license")}
                value={inspection.revision.license ?? t("common.unknown")}
              />
              <InfoRow
                label={t("library.compatibility")}
                value={
                  compatibleAgents.length > 0
                    ? compatibleAgents.join(", ")
                    : t("library.noCompatibleAgent")
                }
              />
              <InfoRow
                label={t("library.risk")}
                value={String(inspection.revision.audit.findings.length)}
              />
              {inspection.revision.audit.findings.map((finding) => (
                <div
                  className="audit-finding"
                  key={`${finding.code}:${finding.path ?? ""}`}
                >
                  <strong>{finding.severity}</strong>
                  <span>{finding.message}</span>
                </div>
              ))}
              <h4>{t("library.files")}</h4>
              {inspection.files.map((file) => (
                <div className="file-summary" key={file.path}>
                  <code>{file.path}</code>
                  <small>
                    {t("library.fileMeta", {
                      size: file.size,
                      hash: file.hash.slice(0, 12),
                    })}
                  </small>
                </div>
              ))}
            </div>
          ) : null}
        </Panel>
        <div className="panel-stack">
          <Panel title={t("library.catalog")} icon={<Globe2 size={18} />}>
            <form
              className="inline-form"
              onSubmit={(event) => void searchCatalog(event)}
            >
              <label className="sr-only" htmlFor="catalog-query">
                {t("common.search")}
              </label>
              <div className="input-with-icon">
                <Search size={16} />
                <input
                  id="catalog-query"
                  disabled={tab === "prompt"}
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={
                    tab === "prompt"
                      ? t("library.promptLocalOnly")
                      : t("library.queryPlaceholder")
                  }
                />
              </div>
              <Button
                type="submit"
                disabled={tab === "prompt"}
                loading={searching}
              >
                {t("common.search")}
              </Button>
            </form>
            {cacheState === "stale_offline" ? (
              <div className="inline-notice">
                <Globe2 size={15} />
                {t("library.staleCache")}
              </div>
            ) : null}
            <div className="stack-list catalog-results">
              {results.map((item) => (
                <div
                  className="catalog-row"
                  key={`${item.provider_id}:${item.id}`}
                >
                  <div>
                    <strong>{item.name}</strong>
                    <small>{item.description ?? item.locator}</small>
                    <div className="chip-row">
                      <span className="chip">{item.provider_id}</span>
                      <span className="chip muted">
                        {item.license ?? t("common.unknown")}
                      </span>
                    </div>
                  </div>
                  <div className="catalog-actions">
                    <span
                      className="verification"
                      data-verified={item.namespace_verified}
                    >
                      {item.namespace_verified
                        ? t("library.verifiedNamespace")
                        : t("library.unverifiedNamespace")}
                    </span>
                    <Button
                      variant="ghost"
                      loading={importing}
                      onClick={() => void importCatalog(item)}
                    >
                      {t("library.importRemote")}
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          </Panel>
          <Panel
            title={t("library.importSource")}
            icon={<PackagePlus size={18} />}
          >
            <form
              className="inline-form"
              onSubmit={(event) => void importAsset(event)}
            >
              <label className="sr-only" htmlFor="asset-source">
                {t("library.importSource")}
              </label>
              <input
                id="asset-source"
                value={source}
                onChange={(event) => setSource(event.target.value)}
                placeholder={
                  tab === "skill"
                    ? t("library.importPlaceholder")
                    : tab === "prompt"
                      ? t("library.promptPlaceholder")
                      : t("library.mcpPlaceholder")
                }
              />
              <Button type="submit" loading={importing}>
                {t("library.importAction")}
              </Button>
            </form>
          </Panel>
        </div>
      </div>
    </section>
  );
}

function AgentsPage({
  agents,
  assets,
  assignments,
  setError,
  setToast,
  reload,
}: {
  agents: AgentInstance[];
  assets: Asset[];
  assignments: Assignment[];
  setError: (error: IpcFailure | null) => void;
  setToast: (message: string | null) => void;
  reload: () => Promise<void>;
}) {
  const { t } = useTranslation();
  const [plan, setPlan] = useState<DeploymentPlan | null>(null);
  const [working, setWorking] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [confirmation, setConfirmation] = useState("");
  const planToggle = async (assignment: Assignment) => {
    setWorking(true);
    try {
      setPlan(
        await api.planAssignmentEnabled(assignment.id, !assignment.enabled),
      );
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  const apply = async () => {
    if (!plan) return;
    setWorking(true);
    try {
      await api.applyPlan(plan.id);
      setPlan(null);
      setConfirming(false);
      setConfirmation("");
      setToast(t("assemble.applied"));
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  return (
    <section>
      <PageHeader title={t("agents.title")} subtitle={t("agents.subtitle")} />
      {agents.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="agent-card-grid">
          {agents.map((agent) => {
            const agentAssignments = assignments.filter(
              (assignment) => assignment.agent_instance_id === agent.id,
            );
            return (
              <article className="agent-card" key={agent.id}>
                <header>
                  <span className="agent-avatar large">
                    {agent.display_name.slice(0, 1)}
                  </span>
                  <div>
                    <h2>{agent.display_name}</h2>
                    <p>{agent.adapter_id}</p>
                  </div>
                  <StatusBadge healthy={agent.health === "healthy"} />
                </header>
                <dl>
                  <InfoRow
                    label={t("agents.version")}
                    value={agent.version ?? t("common.unknown")}
                  />
                  <InfoRow
                    label={t("agents.profile")}
                    value={agent.profile ?? t("common.none")}
                  />
                </dl>
                <h3>{t("agents.paths")}</h3>
                <div className="path-list">
                  {agent.managed_roots.map((path) => (
                    <code key={path}>{path}</code>
                  ))}
                </div>
                <h3>{t("agents.capabilities")}</h3>
                <div className="surface-list">
                  {agent.surfaces.map((surface, index) => (
                    <div key={`${surface.path}:${index}`}>
                      <span>{localizedLabel(t, surface.asset_kind)}</span>
                      <span>
                        {t("agents.scope")}: {surface.scope}
                      </span>
                      <span>
                        {t("agents.mode")}: {localizedLabel(t, surface.mode)}
                      </span>
                    </div>
                  ))}
                </div>
                <h3>{t("agents.assignments")}</h3>
                {agentAssignments.length === 0 ? (
                  <p className="muted-copy">{t("agents.noAssignments")}</p>
                ) : (
                  <div className="assignment-list">
                    {agentAssignments.map((assignment) => (
                      <div className="assignment-row" key={assignment.id}>
                        <div>
                          <strong>
                            {assets.find(
                              (asset) => asset.id === assignment.asset_id,
                            )?.identity.declared_name ?? assignment.asset_id}
                          </strong>
                          <small>{assignment.scope}</small>
                        </div>
                        <Button
                          variant="ghost"
                          loading={working}
                          onClick={() => void planToggle(assignment)}
                        >
                          {assignment.enabled
                            ? t("agents.disable")
                            : t("agents.enable")}
                        </Button>
                      </div>
                    ))}
                  </div>
                )}
              </article>
            );
          })}
        </div>
      )}
      {plan ? (
        <Panel
          title={t("assemble.planTitle")}
          icon={<ShieldCheck size={18} />}
          action={<RiskBadge risk={plan.risk} />}
        >
          <div className="plan-meta">
            <InfoRow label={t("assemble.planId")} value={plan.id} mono />
            <InfoRow
              label={t("assemble.purpose")}
              value={localizedLabel(t, plan.purpose)}
            />
          </div>
          <div className="operation-list">
            {plan.operations.map((operation) => (
              <article key={operation.id}>
                <header>
                  <code>{operation.target_path}</code>
                  <RiskBadge risk={operation.risk} />
                </header>
                <pre>{operation.rendered_diff}</pre>
              </article>
            ))}
          </div>
          <CatalogEffects plan={plan} />
          <Button variant="danger" onClick={() => setConfirming(true)}>
            {t("common.apply")}
          </Button>
        </Panel>
      ) : null}
      {confirming && plan ? (
        <Modal
          title={t("assemble.confirmTitle")}
          onClose={() => setConfirming(false)}
        >
          <p>{t("assemble.confirmBody")}</p>
          <code className="confirmation-hint">…{plan.id.slice(-8)}</code>
          <Field label={t("assemble.confirmation")}>
            <input
              autoFocus
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
            />
          </Field>
          <div className="modal-actions">
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              loading={working}
              disabled={confirmation !== plan.id.slice(-8)}
              onClick={() => void apply()}
            >
              {t("common.apply")}
            </Button>
          </div>
        </Modal>
      ) : null}
    </section>
  );
}

function AssemblePage({
  assets,
  agents,
  setError,
  setToast,
  reload,
}: CommonPageProps) {
  const { t } = useTranslation();
  const [assetId, setAssetId] = useState(assets[0]?.id ?? "");
  const [assetQuery, setAssetQuery] = useState("");
  const [availableAssets, setAvailableAssets] = useState<Asset[]>(assets);
  const [agentId, setAgentId] = useState(agents[0]?.id ?? "");
  const [scope, setScope] = useState("global");
  const [plan, setPlan] = useState<DeploymentPlan | null>(null);
  const [working, setWorking] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [confirmation, setConfirmation] = useState("");
  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void api
        .listAssetsPage({ query: assetQuery, limit: 100 })
        .then((page) => {
          if (cancelled) return;
          setAvailableAssets(page.items);
          if (!page.items.some((asset) => asset.id === assetId)) {
            setAssetId(page.items[0]?.id ?? "");
          }
        })
        .catch((caught: unknown) => {
          if (!cancelled) setError(normalizeError(caught));
        });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [assetId, assetQuery, assets, setError]);
  const generate = async () => {
    if (!assetId || !agentId) return;
    setWorking(true);
    try {
      setPlan(await api.planAssignment(assetId, agentId, scope));
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  const apply = async () => {
    if (!plan || confirmation !== plan.id.slice(-8)) return;
    setWorking(true);
    try {
      await api.applyPlan(plan.id);
      setToast(t("assemble.applied"));
      setConfirming(false);
      setPlan(null);
      setConfirmation("");
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  const losses =
    plan?.operations.flatMap((operation) => operation.compatibility_losses) ??
    [];
  return (
    <section>
      <PageHeader
        title={t("assemble.title")}
        subtitle={t("assemble.subtitle")}
      />
      <Panel title={t("assemble.title")} icon={<Command size={18} />}>
        <div className="form-grid">
          <Field label={t("assemble.assetSearch")}>
            <input
              value={assetQuery}
              onChange={(event) => setAssetQuery(event.target.value)}
              placeholder={t("library.localSearchPlaceholder")}
            />
          </Field>
          <Field label={t("assemble.asset")}>
            <select
              value={assetId}
              onChange={(event) => setAssetId(event.target.value)}
            >
              {availableAssets.map((asset) => (
                <option key={asset.id} value={asset.id}>
                  {asset.identity.declared_name}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("assemble.target")}>
            <select
              value={agentId}
              onChange={(event) => setAgentId(event.target.value)}
            >
              {agents.map((agent) => (
                <option key={agent.id} value={agent.id}>
                  {agent.display_name} · {agent.profile ?? agent.adapter_id}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("assemble.scope")}>
            <input
              value={scope}
              onChange={(event) => setScope(event.target.value)}
            />
          </Field>
          <Button icon={Play} loading={working} onClick={() => void generate()}>
            {t("assemble.generate")}
          </Button>
        </div>
      </Panel>
      {plan ? (
        <Panel
          title={t("assemble.planTitle")}
          icon={<ShieldCheck size={18} />}
          action={<RiskBadge risk={plan.risk} />}
        >
          <div className="plan-meta">
            <InfoRow label={t("assemble.planId")} value={plan.id} mono />
            <InfoRow
              label={t("assemble.purpose")}
              value={localizedLabel(t, plan.purpose)}
            />
            <InfoRow
              label={t("assemble.risk")}
              value={t(`risk.${plan.risk}`)}
            />
          </div>
          <div className="operation-list">
            {plan.operations.map((operation) => (
              <article key={operation.id}>
                <header>
                  <code>{operation.target_path}</code>
                  <RiskBadge risk={operation.risk} />
                </header>
                <div className="operation-details">
                  <span>
                    {t("assemble.operation")}:{" "}
                    {localizedLabel(t, operation.kind)}
                  </span>
                  <span>
                    {t("assemble.rollback")}:{" "}
                    {operation.rollback_object
                      ? t("assemble.rollbackYes")
                      : t("assemble.rollbackNew")}
                  </span>
                </div>
                <pre>{operation.rendered_diff}</pre>
              </article>
            ))}
          </div>
          <CatalogEffects plan={plan} />
          <div className="compatibility-box">
            <h3>{t("assemble.compatibility")}</h3>
            {losses.length === 0 ? (
              <p>{t("assemble.noLoss")}</p>
            ) : (
              losses.map((loss) => (
                <p key={loss.code}>
                  <AlertTriangle size={15} />
                  {loss.message}
                </p>
              ))
            )}
          </div>
          <Button
            variant="danger"
            icon={ShieldCheck}
            onClick={() => setConfirming(true)}
          >
            {t("common.apply")}
          </Button>
        </Panel>
      ) : null}
      {confirming && plan ? (
        <Modal
          title={t("assemble.confirmTitle")}
          onClose={() => setConfirming(false)}
        >
          <p>{t("assemble.confirmBody")}</p>
          <code className="confirmation-hint">…{plan.id.slice(-8)}</code>
          <Field label={t("assemble.confirmation")}>
            <input
              autoFocus
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
            />
          </Field>
          <div className="modal-actions">
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              loading={working}
              disabled={confirmation !== plan.id.slice(-8)}
              onClick={() => void apply()}
            >
              {t("common.apply")}
            </Button>
          </div>
        </Modal>
      ) : null}
    </section>
  );
}

function ConflictsPage({
  conflicts,
  setError,
  setToast,
  reload,
}: CommonPageProps & { conflicts: Conflict[] }) {
  const { t } = useTranslation();
  const [selectedId, setSelectedId] = useState(conflicts[0]?.id ?? "");
  const [action, setAction] = useState<ResolutionAction | "">("");
  const [working, setWorking] = useState(false);
  const [plan, setPlan] = useState<DeploymentPlan | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [confirmation, setConfirmation] = useState("");
  const [renameTo, setRenameTo] = useState("");
  const [backupId, setBackupId] = useState("");
  const [mergedContent, setMergedContent] = useState("");
  const [fileChoices, setFileChoices] = useState<
    Record<string, { choice: FileResolutionChoice; merged: string }>
  >({});
  const selected =
    conflicts.find((conflict) => conflict.id === selectedId) ?? conflicts[0];
  useEffect(() => {
    if (!selectedId && conflicts[0]) setSelectedId(conflicts[0].id);
  }, [conflicts, selectedId]);
  const resolve = async () => {
    if (!selected || !action) return;
    setWorking(true);
    try {
      const request: ConflictResolutionRequest = {
        action,
        files: selected.affected.map((path) => ({
          path,
          choice: fileChoices[path]?.choice ?? "keep_rigdeck",
          ...(fileChoices[path]?.choice === "merged"
            ? { merged_content: fileChoices[path].merged }
            : {}),
        })),
        ...(action === "rename_and_coexist" ? { rename_to: renameTo } : {}),
        ...(action === "restore_backup" ? { backup_id: backupId } : {}),
        ...(action === "three_way_merge" && mergedContent
          ? { merged_content: mergedContent }
          : {}),
      };
      if (action !== "per_file_selection") request.files = [];
      const next = await api.resolveConflict(selected.id, request);
      setPlan(next);
      setToast(t("conflicts.planned"));
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  const apply = async () => {
    if (!plan) return;
    setWorking(true);
    try {
      await api.applyPlan(plan.id);
      setConfirming(false);
      setConfirmation("");
      setPlan(null);
      setToast(t("assemble.applied"));
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  const resolutionInputValid =
    Boolean(action) &&
    (action !== "rename_and_coexist" || renameTo.trim().length > 0) &&
    (action !== "restore_backup" || backupId.trim().length > 0) &&
    (action !== "per_file_selection" ||
      selected?.affected.every((path) => {
        const choice = fileChoices[path]?.choice ?? "keep_rigdeck";
        return choice !== "merged" || Boolean(fileChoices[path]?.merged);
      }));
  return (
    <section>
      <PageHeader
        title={t("conflicts.title")}
        subtitle={t("conflicts.subtitle")}
      />
      {conflicts.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="conflict-layout">
          <div className="conflict-list">
            {conflicts.map((conflict) => (
              <button
                type="button"
                key={conflict.id}
                data-active={selected?.id === conflict.id}
                onClick={() => {
                  setSelectedId(conflict.id);
                  setAction("");
                  setPlan(null);
                  setRenameTo("");
                  setBackupId("");
                  setMergedContent("");
                  setFileChoices({});
                }}
              >
                <TriangleAlert size={17} />
                <span>
                  <strong>{localizedLabel(t, conflict.kind)}</strong>
                  <small>{conflict.cause}</small>
                </span>
                <ChevronRight size={16} />
              </button>
            ))}
          </div>
          {selected ? (
            <Panel
              title={localizedLabel(t, selected.kind)}
              icon={<TriangleAlert size={18} />}
            >
              <InfoRow label={t("conflicts.cause")} value={selected.cause} />
              <InfoRow label={t("conflicts.risk")} value={selected.risk} />
              <div className="affected-list">
                <h3>{t("conflicts.affected")}</h3>
                {selected.affected.map((item) => (
                  <code key={item}>{item}</code>
                ))}
              </div>
              <div className="three-way">
                <DiffColumn
                  title={t("conflicts.base")}
                  value={selected.baseline_hash ?? selected.id.slice(0, 16)}
                />
                <DiffColumn
                  title={t("conflicts.rigdeck")}
                  value={selected.cause}
                />
                <DiffColumn
                  title={t("conflicts.agent")}
                  value={selected.current_hash ?? selected.risk}
                />
              </div>
              <Field label={t("conflicts.actions")}>
                <select
                  value={action}
                  onChange={(event) => {
                    setAction(event.target.value as ResolutionAction | "");
                    setPlan(null);
                    setRenameTo("");
                    setBackupId("");
                    setMergedContent("");
                    setFileChoices({});
                  }}
                >
                  <option value="">—</option>
                  {selected.actions.map((value) => (
                    <option key={value} value={value}>
                      {localizedLabel(t, value)}
                    </option>
                  ))}
                </select>
              </Field>
              {action === "rename_and_coexist" ? (
                <Field label={t("conflicts.renameTo")}>
                  <input
                    value={renameTo}
                    onChange={(event) => setRenameTo(event.target.value)}
                    placeholder={t("conflicts.renamePlaceholder")}
                  />
                </Field>
              ) : null}
              {action === "restore_backup" ? (
                <Field label={t("conflicts.backupId")}>
                  <input
                    value={backupId}
                    onChange={(event) => setBackupId(event.target.value)}
                    placeholder={t("conflicts.backupPlaceholder")}
                  />
                </Field>
              ) : null}
              {action === "three_way_merge" ? (
                <Field label={t("conflicts.mergedContent")}>
                  <textarea
                    rows={8}
                    value={mergedContent}
                    onChange={(event) => setMergedContent(event.target.value)}
                    placeholder={t("conflicts.mergedPlaceholder")}
                  />
                </Field>
              ) : null}
              {action === "per_file_selection" ? (
                <div className="per-file-resolutions">
                  {selected.affected.map((path) => {
                    const current = fileChoices[path] ?? {
                      choice: "keep_rigdeck" as FileResolutionChoice,
                      merged: "",
                    };
                    return (
                      <div key={path} className="per-file-resolution">
                        <code>{path}</code>
                        <select
                          aria-label={t("conflicts.fileChoice", { path })}
                          value={current.choice}
                          onChange={(event) =>
                            setFileChoices((previous) => ({
                              ...previous,
                              [path]: {
                                ...current,
                                choice: event.target
                                  .value as FileResolutionChoice,
                              },
                            }))
                          }
                        >
                          <option value="keep_rigdeck">
                            {t("conflicts.keepRigdeck")}
                          </option>
                          <option value="keep_agent">
                            {t("conflicts.keepAgent")}
                          </option>
                          <option value="merged">
                            {t("conflicts.useMerged")}
                          </option>
                        </select>
                        {current.choice === "merged" ? (
                          <textarea
                            rows={6}
                            value={current.merged}
                            onChange={(event) =>
                              setFileChoices((previous) => ({
                                ...previous,
                                [path]: {
                                  ...current,
                                  merged: event.target.value,
                                },
                              }))
                            }
                            placeholder={t("conflicts.mergedPlaceholder")}
                          />
                        ) : null}
                      </div>
                    );
                  })}
                </div>
              ) : null}
              <Button
                variant="danger"
                loading={working}
                disabled={!resolutionInputValid}
                onClick={() => void resolve()}
              >
                {t("conflicts.resolve")}
              </Button>
            </Panel>
          ) : null}
        </div>
      )}
      {plan ? (
        <Panel
          title={t("assemble.planTitle")}
          icon={<ShieldCheck size={18} />}
          action={<RiskBadge risk={plan.risk} />}
        >
          <div className="plan-meta">
            <InfoRow label={t("assemble.planId")} value={plan.id} mono />
            <InfoRow
              label={t("assemble.purpose")}
              value={localizedLabel(t, plan.purpose)}
            />
          </div>
          <div className="operation-list">
            {plan.operations.map((operation) => (
              <article key={operation.id}>
                <header>
                  <code>{operation.target_path}</code>
                  <RiskBadge risk={operation.risk} />
                </header>
                <div className="operation-details">
                  <span>
                    {t("assemble.operation")}:{" "}
                    {localizedLabel(t, operation.kind)}
                  </span>
                  <span>
                    {t("assemble.rollback")}:{" "}
                    {operation.rollback_object
                      ? t("assemble.rollbackYes")
                      : t("assemble.rollbackNew")}
                  </span>
                </div>
                <pre>{operation.rendered_diff}</pre>
              </article>
            ))}
          </div>
          <CatalogEffects plan={plan} />
          <Button
            variant="danger"
            icon={ShieldCheck}
            onClick={() => setConfirming(true)}
          >
            {t("common.apply")}
          </Button>
        </Panel>
      ) : null}
      {confirming && plan ? (
        <Modal
          title={t("assemble.confirmTitle")}
          onClose={() => setConfirming(false)}
        >
          <p>{t("assemble.confirmBody")}</p>
          <code className="confirmation-hint">…{plan.id.slice(-8)}</code>
          <Field label={t("assemble.confirmation")}>
            <input
              autoFocus
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
            />
          </Field>
          <div className="modal-actions">
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              loading={working}
              disabled={confirmation !== plan.id.slice(-8)}
              onClick={() => void apply()}
            >
              {t("common.apply")}
            </Button>
          </div>
        </Modal>
      ) : null}
    </section>
  );
}

function ActivityPage({
  activity,
  backups,
  setError,
  setToast,
  reload,
}: CommonPageProps & { activity: AuditEvent[]; backups: BackupManifest[] }) {
  const { t, i18n } = useTranslation();
  const [working, setWorking] = useState(false);
  const createBackup = async () => {
    setWorking(true);
    try {
      await api.createBackup();
      setToast(t("activity.createBackup"));
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  const restore = async (backup: BackupManifest) => {
    if (
      !window.confirm(
        t("activity.restoreConfirm", { id: backup.id.slice(0, 12) }),
      )
    )
      return;
    setWorking(true);
    try {
      await api.restoreBackup(backup.id);
      setToast(t("common.restore"));
      await reload();
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  return (
    <section>
      <PageHeader
        title={t("activity.title")}
        subtitle={t("activity.subtitle")}
      />
      <div className="content-grid two-columns">
        <Panel title={t("activity.timeline")} icon={<Activity size={18} />}>
          {activity.length === 0 ? (
            <EmptyState />
          ) : (
            <div className="timeline">
              {activity.map((event) => (
                <article key={event.id}>
                  <span className="timeline-dot" />
                  <div>
                    <strong>{localizedLabel(t, event.event_type)}</strong>
                    <time>
                      {new Intl.DateTimeFormat(i18n.language, {
                        dateStyle: "medium",
                        timeStyle: "short",
                      }).format(event.created_at_ms)}
                    </time>
                    {event.plan_id ? <code>{event.plan_id}</code> : null}
                    <details className="audit-details">
                      <summary>{t("activity.details")}</summary>
                      <pre>{JSON.stringify(event.details, null, 2)}</pre>
                    </details>
                  </div>
                </article>
              ))}
            </div>
          )}
        </Panel>
        <Panel
          title={t("activity.backups")}
          icon={<DatabaseBackup size={18} />}
          action={
            <Button
              icon={DatabaseBackup}
              loading={working}
              onClick={() => void createBackup()}
            >
              {t("activity.createBackup")}
            </Button>
          }
        >
          {backups.length === 0 ? (
            <EmptyState />
          ) : (
            <div className="stack-list">
              {backups.map((backup) => (
                <div className="backup-row" key={backup.id}>
                  <ArchiveRestore size={18} />
                  <div>
                    <code>{backup.id.slice(0, 16)}</code>
                    <small>
                      {t("activity.objectCount", {
                        count: backup.objects.length,
                      })}
                    </small>
                  </div>
                  <Button
                    variant="ghost"
                    disabled={working}
                    onClick={() => void restore(backup)}
                  >
                    {t("common.restore")}
                  </Button>
                </div>
              ))}
            </div>
          )}
        </Panel>
      </div>
    </section>
  );
}

function SettingsPage({
  setError,
}: {
  setError: (error: IpcFailure | null) => void;
}) {
  const { t, i18n } = useTranslation();
  const [theme, setTheme] = useState<Theme>(
    (localStorage.getItem("rigdeck-theme") as Theme | null) ?? "system",
  );
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [working, setWorking] = useState(false);
  useEffect(() => {
    applyTheme(theme);
    localStorage.setItem("rigdeck-theme", theme);
  }, [theme]);
  const language = i18n.language.startsWith("en") ? "en" : "zh";
  const changeLanguage = (value: string) => {
    void i18n.changeLanguage(value);
    localStorage.setItem("rigdeck-language", value);
  };
  const runDoctor = async () => {
    setWorking(true);
    try {
      setDoctor(await api.doctor());
    } catch (caught: unknown) {
      setError(normalizeError(caught));
    } finally {
      setWorking(false);
    }
  };
  const themes: Array<[Theme, typeof Sun]> = [
    ["system", Sparkles],
    ["porcelain", Sun],
    ["obsidian", Moon],
    ["aurora", Sparkles],
  ];
  return (
    <section>
      <PageHeader
        title={t("settings.title")}
        subtitle={t("settings.subtitle")}
      />
      <div className="content-grid two-columns">
        <Panel title={t("settings.language")} icon={<Languages size={18} />}>
          <div className="choice-grid">
            <button
              type="button"
              data-active={language === "zh"}
              onClick={() => changeLanguage("zh")}
            >
              {t("settings.languageZh")}
            </button>
            <button
              type="button"
              data-active={language === "en"}
              onClick={() => changeLanguage("en")}
            >
              {t("settings.languageEn")}
            </button>
          </div>
        </Panel>
        <Panel title={t("settings.theme")} icon={<Sun size={18} />}>
          <div className="choice-grid themes">
            {themes.map(([value, Icon]) => (
              <button
                type="button"
                data-active={theme === value}
                key={value}
                onClick={() => setTheme(value)}
              >
                <Icon size={17} />
                {t(`settings.${value}`)}
              </button>
            ))}
          </div>
        </Panel>
        <Panel
          title={t("settings.diagnostics")}
          icon={<Wrench size={18} />}
          action={
            <Button loading={working} onClick={() => void runDoctor()}>
              {t("settings.runDoctor")}
            </Button>
          }
        >
          {doctor ? (
            <div className="stack-list">
              {doctor.checks.map((check) => (
                <div className="doctor-row" key={check.id}>
                  <StatusBadge healthy={check.healthy} />
                  <div>
                    <strong>{check.id}</strong>
                    <small>{check.message}</small>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <p className="muted-copy">{t("settings.privacy")}</p>
          )}
        </Panel>
        <Panel title={t("settings.security")} icon={<ShieldCheck size={18} />}>
          <p className="muted-copy">{t("settings.privacy")}</p>
        </Panel>
      </div>
    </section>
  );
}

function Panel({
  title,
  icon,
  action,
  children,
}: {
  title: string;
  icon?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <article className="panel">
      <header className="panel-header">
        <div>
          {icon}
          <h2>{title}</h2>
        </div>
        {action}
      </header>
      <div className="panel-body">{children}</div>
    </article>
  );
}
function Button({
  children,
  icon: Icon,
  loading,
  variant = "primary",
  type = "button",
  ...props
}: {
  children: ReactNode;
  icon?: typeof Play;
  loading?: boolean;
  variant?: "primary" | "ghost" | "danger";
  type?: "button" | "submit";
} & Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, "type">) {
  return (
    <button type={type} className={`button ${variant}`} {...props}>
      {loading ? (
        <LoaderCircle className="spin" size={16} />
      ) : Icon ? (
        <Icon size={16} />
      ) : null}
      <span>{children}</span>
    </button>
  );
}
function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}
function InfoRow({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="info-row">
      <dt>{label}</dt>
      <dd className={mono ? "mono" : undefined}>{value}</dd>
    </div>
  );
}
function StatusBadge({ healthy }: { healthy: boolean }) {
  const { t } = useTranslation();
  return (
    <span className="status-badge" data-healthy={healthy}>
      {healthy ? <Check size={13} /> : <AlertTriangle size={13} />}
      {healthy ? t("common.healthy") : t("common.attention")}
    </span>
  );
}
function RiskBadge({ risk }: { risk: RiskLevel }) {
  const { t } = useTranslation();
  return (
    <span className="risk-badge" data-risk={risk}>
      {t(`risk.${risk}`)}
    </span>
  );
}
function EmptyState() {
  const { t } = useTranslation();
  return (
    <div className="empty-state">
      <Boxes size={28} />
      <strong>{t("state.emptyTitle")}</strong>
      <p>{t("state.emptyBody")}</p>
    </div>
  );
}
function LoadingState() {
  const { t } = useTranslation();
  return (
    <div className="full-state">
      <LoaderCircle className="spin" size={28} />
      <p>{t("common.loading")}</p>
    </div>
  );
}
function ErrorState({
  error,
  onRetry,
}: {
  error: IpcFailure;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  const key = error.code.includes("offline")
    ? "offline"
    : error.code.includes("rate")
      ? "rate"
      : error.code.includes("permission") || error.code.includes("secret")
        ? "permission"
        : error.code.includes("unsupported")
          ? "unsupported"
          : error.code.includes("manual")
            ? "manual"
            : "failure";
  return (
    <div className="full-state error">
      <AlertTriangle size={30} />
      <h1>{t(`state.${key}Title`)}</h1>
      <p>{error.message}</p>
      {["offline", "rate", "permission"].includes(key) ? (
        <small>{t(`state.${key}Body`)}</small>
      ) : null}
      <Button onClick={onRetry}>{t("common.retry")}</Button>
    </div>
  );
}
function Modal({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const dialog = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const element = dialog.current;
    if (!element) return;
    const focusable = () => [
      ...element.querySelectorAll<HTMLElement>(
        "button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
      ),
    ];
    if (!element.contains(document.activeElement)) focusable()[0]?.focus();
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab") return;
      const items = focusable();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("keydown", handleKey);
      previous?.focus();
    };
  }, [onClose]);
  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialog}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="modal-title"
      >
        <header>
          <h2 id="modal-title">{title}</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("common.close")}
          >
            <X size={18} />
          </button>
        </header>
        {children}
      </div>
    </div>
  );
}
function DiffColumn({ title, value }: { title: string; value: string }) {
  return (
    <div>
      <strong>{title}</strong>
      <pre>{value}</pre>
    </div>
  );
}

function CatalogEffects({ plan }: { plan: DeploymentPlan }) {
  const { t } = useTranslation();
  if (plan.catalog_effects.length === 0) return null;
  return (
    <div className="catalog-effects">
      <h3>{t("assemble.catalogEffects")}</h3>
      {plan.catalog_effects.map((effect, index) => (
        <pre key={`${effect.kind}:${index}`}>
          {JSON.stringify(effect, null, 2)}
        </pre>
      ))}
    </div>
  );
}

function localizedLabel(t: TFunction, value: string): string {
  return t(`labels.${value}`, { defaultValue: value });
}
function applyTheme(theme: Theme) {
  const root = document.documentElement;
  root.classList.remove("theme-porcelain", "theme-obsidian", "theme-aurora");
  const selected =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "obsidian"
        : "porcelain"
      : theme;
  root.classList.add(`theme-${selected}`);
  root.dataset.themePreference = theme;
}
