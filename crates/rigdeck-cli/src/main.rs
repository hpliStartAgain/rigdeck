use std::{
    io::{IsTerminal, Read},
    process::ExitCode,
};

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
use rigdeck_adapter_sdk::{
    pack_adapter, scaffold_adapter, test_adapter_package, validate_adapter_package, AgentAdapter,
};
use rigdeck_adapters::{BuiltinAdapter, StartupRefreshOutcome};
use rigdeck_core::{ConflictResolutionRequest, DriftState, FileResolution, ResolutionAction};
use rigdeck_service::{home, AppPaths, RigDeckService, ServiceError};
use serde::Serialize;

const EXIT_SUCCESS: u8 = 0;
const EXIT_DRIFT: u8 = 10;
const EXIT_CONFLICT: u8 = 20;
const EXIT_COMPATIBILITY: u8 = 30;
const EXIT_INVALID_PLAN: u8 = 40;
const EXIT_PERMISSION: u8 = 50;
const EXIT_INTERNAL: u8 = 70;

/// RigDeck CLI：统一管理 Agent Skills、提示规则与 MCP Server。
///
/// One deck. Every agent, perfectly equipped.
#[derive(Debug, Parser)]
#[command(name = "rigdeck", version, about, long_about)]
struct Cli {
    /// 输出稳定、带版本的 JSON，而不是人类可读文本。
    #[arg(long, global = true)]
    json: bool,

    /// 覆盖应用数据目录，便于便携部署与隔离测试。
    #[arg(long, global = true, env = "RIGDECK_DATA_DIR")]
    data_dir: Option<Utf8PathBuf>,

    /// 当前项目根；默认是工作目录。
    #[arg(long, global = true)]
    project_root: Option<Utf8PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 检测已安装的 Agent 实例。
    Agents {
        #[command(subcommand)]
        action: AgentsAction,
    },
    /// 从 Agent 原生状态完整刷新本地库存。
    Refresh,
    /// 管理系统钥匙串中的 SecretRef；值从不通过命令行参数传递。
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },
    /// 搜索 Skill 或 MCP Server。
    Search {
        #[command(subcommand)]
        kind: SearchKind,
    },
    /// 查看已导入资产及当前不可变修订。
    Inspect { asset: String },
    /// 导入 Skill、Prompt 或规范化 MCP Server JSON。
    Add {
        source: String,
        /// 资产类型；Skill 仍支持本地目录/归档、skills.sh 和 GitHub。
        #[arg(long, value_enum, default_value_t = AddKind::Skill)]
        kind: AddKind,
        /// Prompt 声明名；默认使用文件名。
        #[arg(long)]
        name: Option<String>,
        /// Prompt 目标作用域，可重复或用逗号分隔。
        #[arg(long, value_delimiter = ',')]
        scope: Vec<String>,
        /// 确认写入 RigDeck 本地库。
        #[arg(long)]
        yes: bool,
    },
    /// 为资产和 Agent 生成部署计划；不会修改 Agent 文件。
    Assign {
        asset: String,
        #[arg(long)]
        agent: String,
        #[arg(long, default_value = "global")]
        scope: String,
    },
    /// 列出分配，或为单个分配生成启用/停用计划。
    Assignments {
        #[command(subcommand)]
        action: AssignmentAction,
    },
    /// 列出待应用计划。
    Plan,
    /// 应用一个已经预览过的显式计划。
    Apply {
        plan_id: String,
        /// 机器调用必须再次提供同一计划 ID，防止参数拼接错位。
        #[arg(long)]
        plan: Option<String>,
        /// 非交互确认。
        #[arg(long)]
        yes: bool,
    },
    /// 显示 Agent、资产、分配和冲突总览。
    Status,
    /// 管理冲突记录。
    Conflicts {
        #[command(subcommand)]
        action: ConflictsAction,
    },
    /// 刷新来源并为可更新资产生成计划。
    Update {
        /// 非交互确认。
        #[arg(long)]
        yes: bool,
    },
    /// 为资产生成精确卸载计划。
    Remove {
        asset: String,
        /// 非交互确认。
        #[arg(long)]
        yes: bool,
    },
    /// 把资产固定在当前修订。
    Pin { asset: String },
    /// 解除资产固定。
    Unpin { asset: String },
    /// 归档资产；保留修订和备份，可恢复。
    Archive { asset: String },
    /// 从归档恢复资产。
    RestoreAsset { asset: String },
    /// 创建 SQLite 与加密对象库的一致性备份。
    Backup,
    /// 从已验证备份恢复；恢复前自动创建 recovery 备份。
    Restore {
        backup_id: String,
        #[arg(long)]
        yes: bool,
    },
    /// 运行数据库、对象库、Adapter 与路径诊断。
    Doctor,
    /// 管理 Adapter 开发包。
    Adapter {
        #[command(subcommand)]
        action: AdapterAction,
    },
    /// 导出不含明文 secret 的可移植配置包。
    Export { output: Utf8PathBuf },
    /// 导入配置包。
    Import {
        input: Utf8PathBuf,
        #[arg(long)]
        yes: bool,
    },
    /// 从旧 agent-skill-registry 一次性导入 Skill。
    ImportRegistry {
        path: Utf8PathBuf,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AgentsAction {
    /// 检测全部七种 Agent 的所有实例。
    Detect,
}

#[derive(Debug, Subcommand)]
enum SecretAction {
    /// 从标准输入读取 secret 并写入系统钥匙串。
    Set {
        reference: String,
        /// 强制确认值来自标准输入；不支持明文参数。
        #[arg(long)]
        stdin: bool,
        /// 确认钥匙串写入。
        #[arg(long)]
        yes: bool,
    },
    /// 检查 SecretRef 是否存在，不显示值。
    Check { reference: String },
    /// 删除 SecretRef；操作幂等。
    Delete {
        reference: String,
        /// 确认钥匙串删除。
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AssignmentAction {
    /// 列出全部分配及当前启用状态。
    List,
    /// 生成重新启用计划。
    Enable {
        assignment: String,
        #[arg(long)]
        yes: bool,
    },
    /// 生成精确停用计划。
    Disable {
        assignment: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SearchKind {
    /// 搜索 skills.sh。
    Skill {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// 搜索 Official MCP Registry preview。
    Mcp {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AddKind {
    /// Agent Skill。
    Skill,
    /// Prompt、Rule 或 instruction 文本。
    Prompt,
    /// 规范化 MCP Server JSON。
    Mcp,
    /// Official MCP Registry 中唯一的 Streamable HTTP remote。
    McpRegistry,
}

impl std::fmt::Display for AddKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Skill => "skill",
            Self::Prompt => "prompt",
            Self::Mcp => "mcp",
            Self::McpRegistry => "mcp-registry",
        })
    }
}

#[derive(Debug, Subcommand)]
enum ConflictsAction {
    /// 列出未解决冲突。
    List,
    /// 显示冲突详情。
    Show { id: String },
    /// 记录一个在冲突自身允许列表中的解决动作。
    Resolve {
        id: String,
        #[arg(long)]
        action: ResolutionChoice,
        /// rename-and-coexist 使用的新资产名。
        #[arg(long)]
        rename_to: Option<String>,
        /// restore-backup 使用的完整备份 ID。
        #[arg(long)]
        backup_id: Option<String>,
        /// three-way-merge 的人工合并 UTF-8 文件；省略时尝试自动合并。
        #[arg(long)]
        merged_file: Option<Utf8PathBuf>,
        /// per-file-selection 的 JSON 文件，内容为 FileResolution 数组。
        #[arg(long)]
        selections_file: Option<Utf8PathBuf>,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ResolutionChoice {
    KeepRigdeckRevision,
    ImportAgentRevision,
    KeepAgentFork,
    RenameAndCoexist,
    ThreeWayMerge,
    PerFileSelection,
    AbandonPlan,
    RestoreBackup,
}

impl From<ResolutionChoice> for ResolutionAction {
    fn from(value: ResolutionChoice) -> Self {
        // Rust 提示：`match` 必须覆盖枚举的每个分支。以后领域枚举新增动作时，
        // 编译器会迫使这里同步更新，因此 CLI 不会悄悄漏掉新能力。
        match value {
            ResolutionChoice::KeepRigdeckRevision => Self::KeepRigdeckRevision,
            ResolutionChoice::ImportAgentRevision => Self::ImportAgentRevision,
            ResolutionChoice::KeepAgentFork => Self::KeepAgentFork,
            ResolutionChoice::RenameAndCoexist => Self::RenameAndCoexist,
            ResolutionChoice::ThreeWayMerge => Self::ThreeWayMerge,
            ResolutionChoice::PerFileSelection => Self::PerFileSelection,
            ResolutionChoice::AbandonPlan => Self::AbandonPlan,
            ResolutionChoice::RestoreBackup => Self::RestoreBackup,
        }
    }
}

#[derive(Debug, Subcommand)]
enum AdapterAction {
    /// 列出内置 Adapter。
    List,
    /// 验证 Adapter 包。
    Validate { path: Utf8PathBuf },
    /// 对 Windows/macOS 夹具运行契约测试。
    Test { path: Utf8PathBuf },
    /// 创建 Adapter 脚手架。
    Scaffold {
        id: String,
        #[arg(long)]
        output: Option<Utf8PathBuf>,
    },
    /// 创建确定性分发 bundle。
    Pack {
        path: Utf8PathBuf,
        #[arg(long)]
        output: Option<Utf8PathBuf>,
    },
}

#[derive(Debug, Serialize)]
struct JsonEnvelope<T: Serialize> {
    schema_version: u32,
    ok: bool,
    data: T,
}

#[derive(Debug, Serialize)]
struct JsonErrorEnvelope {
    schema_version: u32,
    ok: bool,
    error: JsonError,
}

#[derive(Debug, Serialize)]
struct JsonError {
    code: &'static str,
    message: String,
    exit_code: u8,
}

#[derive(Debug, Serialize)]
struct AdapterSummary {
    adapter_id: String,
    version: String,
    display_name: String,
    official_docs: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error("操作未确认：{0}")]
    Confirmation(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Tokio 的属性宏会把这个异步函数展开成普通 `main`，并负责启动运行时。
/// 这样网络 provider 可以 `.await`，而本地 Planner/SQLite 仍保持同步、可预测。
#[tokio::main]
async fn main() -> ExitCode {
    rigdeck_security::install_redacted_panic_hook();
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli).await {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let (code, machine_code) = classify_error(&error);
            let safe_message = safe_error_message(&error);
            if json {
                let envelope = JsonErrorEnvelope {
                    schema_version: 1,
                    ok: false,
                    error: JsonError {
                        code: machine_code,
                        message: safe_message,
                        exit_code: code,
                    },
                };
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&envelope)
                        .unwrap_or_else(|_| "{\"schema_version\":1,\"ok\":false}".to_owned())
                );
            } else {
                eprintln!("错误：{safe_message}");
            }
            ExitCode::from(code)
        }
    }
}

async fn run(cli: Cli) -> Result<u8, CliError> {
    let Cli {
        json,
        data_dir,
        project_root,
        command,
    } = cli;

    // Adapter 开发工具不需要打开数据库或钥匙串，离线打包和验证因而保持独立。
    if let Commands::Adapter { action } = command {
        run_adapter(action, json)?;
        return Ok(EXIT_SUCCESS);
    }

    let paths = data_dir.map_or_else(AppPaths::discover, |root| Ok(AppPaths::for_root(root)))?;
    let mut service = RigDeckService::open(paths)?;
    let project_root = project_root.or_else(current_project_root);

    let exit_code = match command {
        Commands::Agents {
            action: AgentsAction::Detect,
        }
        | Commands::Refresh => {
            let outcome = service.refresh(home()?, project_root)?;
            print_refresh(&outcome, json)?;
            refresh_exit_code(&outcome)
        }
        Commands::Secret { action } => {
            match action {
                SecretAction::Set {
                    reference,
                    stdin,
                    yes,
                } => {
                    require_confirmation(yes, json, "保存 SecretRef")?;
                    if !stdin || std::io::stdin().is_terminal() {
                        return Err(ServiceError::InvalidInput(
                            "secret 值只能通过管道或重定向配合 --stdin 输入".to_owned(),
                        )
                        .into());
                    }
                    let bytes = read_secret_stdin()?;
                    let saved = service.set_secret(&reference, bytes)?;
                    if json {
                        print_json(&serde_json::json!({
                            "secret_ref": saved.as_str(),
                            "present": true
                        }))?;
                    } else {
                        println!("已保存 SecretRef：{}", saved.as_str());
                    }
                }
                SecretAction::Check { reference } => {
                    let present = service.has_secret(&reference)?;
                    if json {
                        print_json(&serde_json::json!({
                            "secret_ref": reference,
                            "present": present
                        }))?;
                    } else {
                        println!(
                            "SecretRef {reference}：{}",
                            if present { "存在" } else { "不存在" }
                        );
                    }
                }
                SecretAction::Delete { reference, yes } => {
                    require_confirmation(yes, json, "删除 SecretRef")?;
                    let deleted = service.delete_secret(&reference)?;
                    if json {
                        print_json(&serde_json::json!({
                            "secret_ref": deleted.as_str(),
                            "present": false
                        }))?;
                    } else {
                        println!("已删除 SecretRef：{}", deleted.as_str());
                    }
                }
            }
            EXIT_SUCCESS
        }
        Commands::Search { kind } => {
            let result = match kind {
                SearchKind::Skill { query, limit } => service.search_skills(&query, limit).await?,
                SearchKind::Mcp { query, limit } => service.search_mcp(&query, limit).await?,
            };
            if json {
                print_json(&result)?;
            } else {
                println!("类型\t名称\t版本\tProvider\t定位符");
                for item in result.items {
                    println!(
                        "{:?}\t{}\t{}\t{}\t{}",
                        item.kind,
                        item.name,
                        item.version.as_deref().unwrap_or("-"),
                        item.provider_id,
                        item.locator
                    );
                }
            }
            EXIT_SUCCESS
        }
        Commands::Inspect { asset } => {
            let inspection = service.inspect(&asset)?;
            if json {
                print_json(&inspection)?;
            } else {
                println!("资产 ID：{}", inspection.asset.id);
                println!("名称：{}", inspection.asset.identity.declared_name);
                println!("类型：{:?}", inspection.asset.kind);
                println!("修订：{}", inspection.revision.id);
                println!("来源：{}", inspection.revision.source.locator);
                println!(
                    "许可证：{}",
                    inspection.revision.license.as_deref().unwrap_or("未知")
                );
                println!("审计发现：{} 项", inspection.revision.audit.findings.len());
            }
            EXIT_SUCCESS
        }
        Commands::Add {
            source,
            kind,
            name,
            scope,
            yes,
        } => {
            require_confirmation(yes, json, "导入资产")?;
            let inspection = match kind {
                AddKind::Skill => service.add_skill_source(&source).await?,
                AddKind::Prompt => service.add_local_prompt(
                    Utf8PathBuf::from(&source).as_path(),
                    name.as_deref(),
                    &scope,
                )?,
                AddKind::Mcp => {
                    if name.is_some() || !scope.is_empty() {
                        return Err(ServiceError::InvalidInput(
                            "--name/--scope 仅用于 Prompt".to_owned(),
                        )
                        .into());
                    }
                    service.add_local_mcp(Utf8PathBuf::from(&source).as_path())?
                }
                AddKind::McpRegistry => {
                    if !scope.is_empty() {
                        return Err(
                            ServiceError::InvalidInput("--scope 仅用于 Prompt".to_owned()).into(),
                        );
                    }
                    service.add_mcp_registry(&source, name.as_deref()).await?
                }
            };
            if json {
                print_json(&inspection)?;
            } else {
                println!(
                    "已导入 {}（资产 {}，修订 {}）",
                    inspection.asset.identity.declared_name,
                    inspection.asset.id,
                    inspection.revision.id
                );
            }
            EXIT_SUCCESS
        }
        Commands::Assign {
            asset,
            agent,
            scope,
        } => {
            let plan = service.plan_assignment(&asset, &agent, &scope)?;
            print_plan(&plan, json)?;
            EXIT_SUCCESS
        }
        Commands::Assignments { action } => {
            match action {
                AssignmentAction::List => {
                    let assignments = service.list_assignments()?;
                    if json {
                        print_json(&assignments)?;
                    } else {
                        println!("分配 ID\t资产\tAgent\t作用域\t启用");
                        for assignment in assignments {
                            println!(
                                "{}\t{}\t{}\t{}\t{}",
                                assignment.id,
                                assignment.asset_id,
                                assignment.agent_instance_id,
                                assignment.scope,
                                if assignment.enabled { "是" } else { "否" }
                            );
                        }
                    }
                }
                AssignmentAction::Enable { assignment, yes } => {
                    require_confirmation(yes, json, "生成启用计划")?;
                    let plan = service.plan_assignment_enabled(&assignment, true)?;
                    print_plan(&plan, json)?;
                }
                AssignmentAction::Disable { assignment, yes } => {
                    require_confirmation(yes, json, "生成停用计划")?;
                    let plan = service.plan_assignment_enabled(&assignment, false)?;
                    print_plan(&plan, json)?;
                }
            }
            EXIT_SUCCESS
        }
        Commands::Plan => {
            let plans = service.list_plans(Some("pending"))?;
            if json {
                print_json(&plans)?;
            } else {
                println!("计划 ID\t风险\t操作数\t创建时间(ms)");
                for plan in plans {
                    println!(
                        "{}\t{:?}\t{}\t{}",
                        plan.id,
                        plan.risk,
                        plan.operations.len(),
                        plan.created_at_ms
                    );
                }
            }
            EXIT_SUCCESS
        }
        Commands::Apply { plan_id, plan, yes } => {
            require_confirmation(yes, json, "应用部署计划")?;
            require_matching_plan(&plan_id, plan.as_deref(), json)?;
            let report = service.apply_plan(&plan_id)?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "计划 {} 已应用，完成 {} 个操作",
                    report.plan_id,
                    report.files.len()
                );
            }
            EXIT_SUCCESS
        }
        Commands::Status => {
            let status = service.status()?;
            if json {
                print_json(&status)?;
            } else {
                println!("Agent：{}", status.agents.len());
                println!("资产：{}", status.asset_count);
                println!("分配：{}", status.assignment_count);
                println!("未解决冲突：{}", status.conflict_count);
            }
            if status.conflict_count > 0 {
                EXIT_CONFLICT
            } else {
                EXIT_SUCCESS
            }
        }
        Commands::Conflicts { action } => match action {
            ConflictsAction::List => {
                let conflicts = service.list_conflicts(Some(false))?;
                if json {
                    print_json(&conflicts)?;
                } else {
                    println!("冲突 ID\t类型\t原因");
                    for conflict in &conflicts {
                        println!("{}\t{:?}\t{}", conflict.id, conflict.kind, conflict.cause);
                    }
                }
                if conflicts.is_empty() {
                    EXIT_SUCCESS
                } else {
                    EXIT_CONFLICT
                }
            }
            ConflictsAction::Show { id } => {
                let conflict = service.conflict(&id)?;
                if json {
                    print_json(&conflict)?;
                } else {
                    println!("冲突 {}：{:?}", conflict.id, conflict.kind);
                    println!("原因：{}", conflict.cause);
                    println!("风险：{}", conflict.risk);
                    println!("影响：{}", conflict.affected.join(", "));
                    println!("可选动作：{:?}", conflict.actions);
                }
                EXIT_CONFLICT
            }
            ConflictsAction::Resolve {
                id,
                action,
                rename_to,
                backup_id,
                merged_file,
                selections_file,
                yes,
            } => {
                require_confirmation(yes, json, "生成冲突解决计划")?;
                let merged_content = merged_file
                    .map(|path| {
                        std::fs::read_to_string(&path).map_err(|error| {
                            ServiceError::InvalidInput(format!("无法读取 {path}：{error}"))
                        })
                    })
                    .transpose()?;
                let files: Vec<FileResolution> = selections_file
                    .map(|path| {
                        std::fs::read(&path)
                            .map_err(|error| {
                                ServiceError::InvalidInput(format!("无法读取 {path}：{error}"))
                            })
                            .and_then(|bytes| {
                                serde_json::from_slice(&bytes).map_err(ServiceError::from)
                            })
                    })
                    .transpose()?
                    .unwrap_or_default();
                let request = ConflictResolutionRequest {
                    action: action.into(),
                    rename_to,
                    backup_id,
                    merged_content,
                    files,
                };
                let plan = service.resolve_conflict_request(&id, request)?;
                print_plan(&plan, json)?;
                EXIT_SUCCESS
            }
        },
        Commands::Backup => {
            let backup = service.create_backup()?;
            if json {
                print_json(&backup)?;
            } else {
                println!(
                    "已创建备份 {}（{} 个对象）",
                    backup.id,
                    backup.objects.len()
                );
            }
            EXIT_SUCCESS
        }
        Commands::Restore { backup_id, yes } => {
            require_confirmation(yes, json, "恢复备份")?;
            let backup = service.restore_backup(&backup_id)?;
            if json {
                print_json(&backup)?;
            } else {
                println!("已恢复备份 {}", backup.id);
            }
            EXIT_SUCCESS
        }
        Commands::Doctor => {
            let report = service.doctor();
            if json {
                print_json(&report)?;
            } else {
                for check in &report.checks {
                    println!(
                        "{}\t{}\t{}",
                        if check.healthy { "通过" } else { "失败" },
                        check.id,
                        check.message
                    );
                }
            }
            if report.healthy {
                EXIT_SUCCESS
            } else {
                EXIT_INTERNAL
            }
        }
        Commands::Update { yes } => {
            require_confirmation(yes, json, "生成更新计划")?;
            let report = service.update_assets().await?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "已检查 {} 个资产：{} 个新修订，{} 个更新计划，{} 个跳过项",
                    report.checked,
                    report.updated.len(),
                    report.plans.len(),
                    report.skipped.len()
                );
                for plan in &report.plans {
                    print_plan(plan, false)?;
                }
            }
            EXIT_SUCCESS
        }
        Commands::Remove { asset, yes } => {
            require_confirmation(yes, json, "生成卸载计划")?;
            let plans = service.plan_remove_asset(&asset)?;
            if json {
                print_json(&plans)?;
            } else {
                for plan in &plans {
                    print_plan(plan, false)?;
                }
            }
            EXIT_SUCCESS
        }
        Commands::Pin { asset } => {
            let result = service.pin_asset(&asset)?;
            if json {
                print_json(&result)?;
            } else {
                println!("资产 {} 已固定到当前修订", asset);
            }
            EXIT_SUCCESS
        }
        Commands::Unpin { asset } => {
            let result = service.unpin_asset(&asset)?;
            if json {
                print_json(&result)?;
            } else {
                println!("资产 {} 已解除固定", asset);
            }
            EXIT_SUCCESS
        }
        Commands::Archive { asset } => {
            let result = service.archive_asset(&asset)?;
            if json {
                print_json(&result)?;
            } else {
                println!("资产 {} 已归档", asset);
            }
            EXIT_SUCCESS
        }
        Commands::RestoreAsset { asset } => {
            let result = service.restore_asset(&asset)?;
            if json {
                print_json(&result)?;
            } else {
                println!("资产 {} 已从归档恢复", asset);
            }
            EXIT_SUCCESS
        }
        Commands::Export { output } => {
            let bundle = service.export_bundle(&output)?;
            if json {
                print_json(&serde_json::json!({
                    "output": output,
                    "asset_count": bundle.assets.len(),
                    "assignment_count": bundle.assignments.len(),
                    "schema_version": bundle.schema_version,
                }))?;
            } else {
                println!(
                    "已导出 {} 个资产和 {} 个分配意图：{}",
                    bundle.assets.len(),
                    bundle.assignments.len(),
                    output
                );
            }
            EXIT_SUCCESS
        }
        Commands::Import { input, yes } => {
            require_confirmation(yes, json, "导入配置包")?;
            let report = service.import_bundle(&input)?;
            if json {
                print_json(&report)?;
            } else {
                println!("已导入 {} 个资产", report.asset_ids.len());
                if !report.pending_assignments.is_empty() {
                    println!(
                        "{} 个分配意图需要在本机重新匹配 Agent",
                        report.pending_assignments.len()
                    );
                }
            }
            EXIT_SUCCESS
        }
        Commands::ImportRegistry { path, yes } => {
            require_confirmation(yes, json, "从旧 Registry 导入")?;
            let report = service.import_legacy_registry(&path)?;
            if json {
                print_json(&report)?;
            } else {
                println!("已导入 {} 个 Skill", report.imported.len());
                if !report.skipped.is_empty() {
                    println!("跳过 {} 个：", report.skipped.len());
                    for item in &report.skipped {
                        println!("  {item}");
                    }
                }
            }
            EXIT_SUCCESS
        }
        Commands::Adapter { .. } => unreachable!("Adapter 已在打开服务前处理"),
    };
    Ok(exit_code)
}

fn current_project_root() -> Option<Utf8PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|path| Utf8PathBuf::from_path_buf(path).ok())
}

fn require_confirmation(yes: bool, json: bool, action: &str) -> Result<(), CliError> {
    if yes {
        return Ok(());
    }
    // `--json` 代表机器调用；stdin 不是终端时也是非交互调用。两者都必须显式 `--yes`。
    if json || !std::io::stdin().is_terminal() {
        return Err(CliError::Confirmation(format!(
            "{action} 需要 --yes；机器调用不会自动确认"
        )));
    }
    Err(CliError::Confirmation(format!(
        "{action} 需要先预览并显式传入 --yes"
    )))
}

fn require_matching_plan(
    positional: &str,
    explicit: Option<&str>,
    json: bool,
) -> Result<(), CliError> {
    if let Some(value) = explicit {
        if value == positional {
            return Ok(());
        }
        return Err(CliError::Confirmation(
            "位置计划 ID 与 --plan 不一致，拒绝应用".to_owned(),
        ));
    }
    if json || !std::io::stdin().is_terminal() {
        return Err(CliError::Confirmation(
            "机器调用必须同时传入匹配的 --plan <id>".to_owned(),
        ));
    }
    Ok(())
}

fn refresh_exit_code(outcome: &StartupRefreshOutcome) -> u8 {
    let mut drift = false;
    for item in outcome
        .instances
        .iter()
        .flat_map(|instance| &instance.report.items)
    {
        if item.conflict.is_some() || item.state == DriftState::Conflict {
            return EXIT_CONFLICT;
        }
        drift |= !matches!(item.state, DriftState::ManagedClean);
    }
    if drift {
        EXIT_DRIFT
    } else {
        EXIT_SUCCESS
    }
}

fn read_secret_stdin() -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(65_537)
        .read_to_end(&mut bytes)
        .map_err(anyhow::Error::from)?;
    if bytes.len() > 65_536 {
        return Err(ServiceError::InvalidInput("secret 超过 65536 字节".to_owned()).into());
    }
    // shell 的 `echo` 通常附带一个行尾；只移除一个 CRLF/LF，不改动其他字节。
    if bytes.ends_with(b"\r\n") {
        bytes.truncate(bytes.len() - 2);
    } else if bytes.ends_with(b"\n") {
        bytes.truncate(bytes.len() - 1);
    }
    Ok(bytes)
}

fn print_refresh(outcome: &StartupRefreshOutcome, json: bool) -> Result<(), CliError> {
    if json {
        print_json(outcome)?;
    } else {
        println!("Adapter\t实例数\t状态");
        for adapter in &outcome.adapters {
            println!(
                "{}\t{}\t{}",
                adapter.adapter_id,
                adapter.instance_count,
                adapter.error.as_deref().unwrap_or("正常")
            );
        }
        let changes: usize = outcome
            .instances
            .iter()
            .map(|instance| instance.report.items.len())
            .sum();
        println!(
            "刷新完成：{} 个实例，{} 条库存记录",
            outcome.instances.len(),
            changes
        );
    }
    Ok(())
}

fn print_plan(plan: &rigdeck_core::DeploymentPlan, json: bool) -> Result<(), CliError> {
    if json {
        print_json(plan)?;
    } else {
        println!("计划 ID：{}", plan.id);
        println!("风险：{:?}", plan.risk);
        println!("目标路径\t操作\t风险\t说明");
        for operation in &plan.operations {
            println!(
                "{}\t{:?}\t{:?}\t{}",
                operation.target_path, operation.kind, operation.risk, operation.rendered_diff
            );
        }
        println!(
            "应用前请检查上述路径与风险，再运行：rigdeck apply {} --yes",
            plan.id
        );
    }
    Ok(())
}

fn run_adapter(action: AdapterAction, json: bool) -> Result<(), CliError> {
    match action {
        AdapterAction::List => {
            let summaries: Vec<_> = BuiltinAdapter::load_all()
                .map_err(anyhow::Error::from)?
                .into_iter()
                .map(|adapter| {
                    let manifest = adapter.describe();
                    AdapterSummary {
                        adapter_id: manifest.adapter_id.clone(),
                        version: manifest.version.clone(),
                        display_name: manifest.display_name.clone(),
                        official_docs: manifest.official_docs.clone(),
                    }
                })
                .collect();
            if json {
                print_json(&summaries)?;
            } else {
                println!("ID\t版本\t名称");
                for item in summaries {
                    println!(
                        "{}\t{}\t{}",
                        item.adapter_id, item.version, item.display_name
                    );
                }
            }
        }
        AdapterAction::Validate { path } => {
            let report = validate_adapter_package(&path).map_err(anyhow::Error::from)?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Adapter {} 验证通过：{} 个文件，{} 字节，hash {}",
                    report.adapter_id, report.file_count, report.total_bytes, report.package_hash
                );
            }
        }
        AdapterAction::Test { path } => {
            let report = test_adapter_package(&path).map_err(anyhow::Error::from)?;
            if json {
                print_json(&report)?;
            } else {
                println!(
                    "Adapter {} 契约测试通过：{}",
                    report.adapter_id,
                    report.fixtures.join(", ")
                );
            }
        }
        AdapterAction::Scaffold { id, output } => {
            let output = output.unwrap_or_else(|| Utf8PathBuf::from(&id));
            let manifest = scaffold_adapter(&output, &id).map_err(anyhow::Error::from)?;
            if json {
                print_json(&serde_json::json!({
                    "adapter_id": manifest.adapter_id,
                    "path": output,
                }))?;
            } else {
                println!("已创建 Adapter {}：{}", manifest.adapter_id, output);
            }
        }
        AdapterAction::Pack { path, output } => {
            let output = output.unwrap_or_else(|| {
                let name = path.file_name().unwrap_or("adapter");
                Utf8PathBuf::from(format!("{name}.rigdeck-adapter"))
            });
            let bundle = pack_adapter(&path, &output).map_err(anyhow::Error::from)?;
            if json {
                print_json(&serde_json::json!({
                    "adapter_id": bundle.adapter_id,
                    "version": bundle.version,
                    "package_hash": bundle.package_hash,
                    "file_count": bundle.files.len(),
                    "output": output,
                }))?;
            } else {
                println!(
                    "已打包 Adapter {}：{}（{} 个文件）",
                    bundle.adapter_id,
                    output,
                    bundle.files.len()
                );
            }
        }
    }
    Ok(())
}

fn print_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let envelope = JsonEnvelope {
        schema_version: 1,
        ok: true,
        data: value,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&envelope).map_err(anyhow::Error::from)?
    );
    Ok(())
}

fn classify_error(error: &CliError) -> (u8, &'static str) {
    match error {
        CliError::Confirmation(_) => (EXIT_INVALID_PLAN, "confirmation_required"),
        CliError::Service(ServiceError::ManualRequired(_)) => {
            (EXIT_COMPATIBILITY, "manual_required")
        }
        CliError::Service(ServiceError::NotFound(_)) => (EXIT_INVALID_PLAN, "not_found"),
        CliError::Service(ServiceError::InvalidInput(_)) => (EXIT_INVALID_PLAN, "invalid_input"),
        CliError::Service(ServiceError::Core(core)) => match core {
            rigdeck_core::CoreError::InvalidPlan(_)
            | rigdeck_core::CoreError::PlanInvalidated { .. }
            | rigdeck_core::CoreError::ObjectUnavailable(_) => (EXIT_INVALID_PLAN, "invalid_plan"),
            rigdeck_core::CoreError::Io { source, .. }
                if source.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                (EXIT_PERMISSION, "permission_denied")
            }
            _ => (EXIT_INTERNAL, "core_error"),
        },
        CliError::Service(ServiceError::Adapter(adapter)) => match adapter.code {
            rigdeck_adapter_sdk::AdapterErrorCode::UnsupportedPlatform
            | rigdeck_adapter_sdk::AdapterErrorCode::UnsupportedCapability
            | rigdeck_adapter_sdk::AdapterErrorCode::ManualRequired => {
                (EXIT_COMPATIBILITY, "compatibility_failure")
            }
            rigdeck_adapter_sdk::AdapterErrorCode::PathViolation
            | rigdeck_adapter_sdk::AdapterErrorCode::InvalidManifest
            | rigdeck_adapter_sdk::AdapterErrorCode::IncompatibleProtocol => {
                (EXIT_INVALID_PLAN, "invalid_plan")
            }
            _ => (EXIT_INTERNAL, "adapter_error"),
        },
        CliError::Service(ServiceError::Registry(_)) => (EXIT_INTERNAL, "registry_error"),
        CliError::Service(ServiceError::Store(_)) => (EXIT_INTERNAL, "store_error"),
        CliError::Service(ServiceError::Secret(_)) => (EXIT_PERMISSION, "secret_store_error"),
        CliError::Service(ServiceError::Cancelled) => (EXIT_INTERNAL, "cancelled"),
        CliError::Service(ServiceError::Json(_)) | CliError::Other(_) => {
            (EXIT_INTERNAL, "internal_error")
        }
    }
}

fn safe_error_message(error: &CliError) -> String {
    rigdeck_security::redact_text(&error.to_string(), &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap_contract_contains_required_commands() {
        use clap::CommandFactory;

        Cli::command().debug_assert();
        for command in [
            "agents",
            "refresh",
            "secret",
            "search",
            "inspect",
            "add",
            "assign",
            "assignments",
            "plan",
            "apply",
            "status",
            "conflicts",
            "update",
            "remove",
            "backup",
            "restore",
            "doctor",
            "adapter",
            "export",
            "import",
        ] {
            assert!(
                Cli::command().find_subcommand(command).is_some(),
                "缺少 {command}"
            );
        }
    }

    #[test]
    fn non_interactive_json_write_fails_closed() {
        let error = require_confirmation(false, true, "测试写入").unwrap_err();
        assert!(matches!(error, CliError::Confirmation(_)));
        assert_eq!(classify_error(&error).0, EXIT_INVALID_PLAN);
    }

    #[test]
    fn machine_apply_requires_matching_explicit_plan() {
        assert!(require_matching_plan("plan-a", None, true).is_err());
        assert!(require_matching_plan("plan-a", Some("plan-b"), true).is_err());
        assert!(require_matching_plan("plan-a", Some("plan-a"), true).is_ok());
    }

    #[test]
    fn secret_set_contract_has_no_plaintext_positional_argument() {
        let parsed = Cli::try_parse_from([
            "rigdeck",
            "secret",
            "set",
            "keychain:test",
            "plaintext-must-not-be-accepted",
            "--yes",
        ]);
        assert!(parsed.is_err());
    }

    #[test]
    fn json_error_message_uses_shared_redactor() {
        let error = CliError::Other(anyhow::anyhow!(
            "request failed\nAuthorization: Bearer cli-json-secret"
        ));
        let message = safe_error_message(&error);
        let envelope = JsonErrorEnvelope {
            schema_version: 1,
            ok: false,
            error: JsonError {
                code: "internal_error",
                message,
                exit_code: EXIT_INTERNAL,
            },
        };
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(!json.contains("cli-json-secret"));
        assert!(json.contains(rigdeck_security::REDACTED));
    }
}
