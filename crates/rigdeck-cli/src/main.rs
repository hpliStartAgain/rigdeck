use clap::{Parser, Subcommand};

/// RigDeck CLI — Manage Agent Skills, prompts, and MCP servers.
///
/// One deck. Every agent, perfectly equipped.
#[derive(Parser)]
#[command(name = "rigdeck", version, about, long_about)]
struct Cli {
    /// Output JSON instead of human-readable tables.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Detect installed agent instances.
    Agents {
        #[command(subcommand)]
        action: Option<AgentsAction>,
    },
    /// Refresh inventory from agent-native state.
    Refresh,
    /// Search for skills or MCP servers.
    Search {
        /// What to search for.
        #[command(subcommand)]
        kind: SearchKind,
    },
    /// Inspect an asset before installing.
    Inspect {
        /// Asset identifier or source URL.
        asset: String,
    },
    /// Add an asset from a source.
    Add {
        /// Source URL or path.
        source: String,
    },
    /// Assign an asset to an agent scope.
    Assign {
        /// Asset identifier.
        asset: String,
        /// Target agent ID.
        #[arg(long)]
        agent: String,
        /// Target scope (global, project, etc.).
        #[arg(long)]
        scope: Option<String>,
    },
    /// Generate a deployment plan.
    Plan,
    /// Apply a deployment plan.
    Apply {
        /// Plan ID to apply.
        plan_id: String,
        /// Non-interactive confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Show current status.
    Status,
    /// Manage conflicts.
    Conflicts {
        #[command(subcommand)]
        action: ConflictsAction,
    },
    /// Update installed assets.
    Update,
    /// Remove an installed asset.
    Remove {
        /// Asset identifier.
        asset: String,
    },
    /// Backup current state.
    Backup,
    /// Restore from a backup.
    Restore {
        /// Backup identifier.
        backup_id: String,
    },
    /// Run diagnostic checks.
    Doctor,
    /// Manage adapters.
    Adapter {
        #[command(subcommand)]
        action: AdapterAction,
    },
    /// Export configuration bundle.
    Export {
        /// Output file path.
        output: String,
    },
    /// Import configuration bundle.
    Import {
        /// Input file path.
        input: String,
    },
}

#[derive(Subcommand)]
enum AgentsAction {
    /// Detect all agent instances.
    Detect,
}

#[derive(Subcommand)]
enum SearchKind {
    /// Search for skills.
    Skill { query: String },
    /// Search for MCP servers.
    Mcp { query: String },
}

#[derive(Subcommand)]
enum ConflictsAction {
    /// List active conflicts.
    List,
    /// Show conflict details.
    Show { id: String },
    /// Resolve a conflict.
    Resolve { id: String },
}

#[derive(Subcommand)]
enum AdapterAction {
    /// List installed adapters.
    List,
    /// Validate an adapter package.
    Validate { path: String },
    /// Test an adapter against fixtures.
    Test { id: String },
    /// Scaffold a new adapter.
    Scaffold { id: String },
    /// Pack an adapter for distribution.
    Pack { path: String },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.json {
        // TODO: JSON output mode
    }

    match cli.command {
        Commands::Agents { action } => {
            println!("rigdeck agents — not yet implemented");
        }
        Commands::Refresh => {
            println!("rigdeck refresh — not yet implemented");
        }
        Commands::Search { kind } => {
            println!("rigdeck search — not yet implemented");
        }
        Commands::Inspect { asset } => {
            println!("rigdeck inspect {asset} — not yet implemented");
        }
        Commands::Add { source } => {
            println!("rigdeck add {source} — not yet implemented");
        }
        Commands::Assign { asset, agent, scope } => {
            println!("rigdeck assign {asset} -> {agent} — not yet implemented");
        }
        Commands::Plan => {
            println!("rigdeck plan — not yet implemented");
        }
        Commands::Apply { plan_id, yes } => {
            println!("rigdeck apply {plan_id} — not yet implemented");
        }
        Commands::Status => {
            println!("rigdeck status — not yet implemented");
        }
        Commands::Conflicts { action } => {
            println!("rigdeck conflicts — not yet implemented");
        }
        Commands::Update => {
            println!("rigdeck update — not yet implemented");
        }
        Commands::Remove { asset } => {
            println!("rigdeck remove {asset} — not yet implemented");
        }
        Commands::Backup => {
            println!("rigdeck backup — not yet implemented");
        }
        Commands::Restore { backup_id } => {
            println!("rigdeck restore {backup_id} — not yet implemented");
        }
        Commands::Doctor => {
            println!("rigdeck doctor — not yet implemented");
        }
        Commands::Adapter { action } => {
            println!("rigdeck adapter — not yet implemented");
        }
        Commands::Export { output } => {
            println!("rigdeck export {output} — not yet implemented");
        }
        Commands::Import { input } => {
            println!("rigdeck import {input} — not yet implemented");
        }
    }

    Ok(())
}
