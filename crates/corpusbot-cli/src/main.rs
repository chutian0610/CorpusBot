use clap::{Parser, Subcommand};
use corpusbot_core::{Template, VERSION};
use corpusbot_store::Workspace;
use serde_json::json;

#[derive(Debug, Parser)]
#[command(
    name = "corpusbot",
    version = VERSION,
    about = "Local knowledge-base engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize a new workspace.
    Init {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long, default_value = "generic")]
        template: String,
    },
    /// Show engine status.
    Status {
        #[arg(long)]
        root: std::path::PathBuf,
    },
    /// Capture a workspace snapshot.
    Snapshot {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long, default_value = "manual snapshot")]
        message: String,
    },
    /// List recent snapshots.
    History {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Restore workspace content to a snapshot.
    Restore {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long)]
        snapshot: String,
        #[arg(long)]
        yes: bool,
    },
}

fn parse_template(value: &str) -> anyhow::Result<Template> {
    match value {
        "generic" => Ok(Template::Generic),
        "research" => Ok(Template::Research),
        other => Err(anyhow::anyhow!("unknown template: {other}")),
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { root, template } => {
            let template = parse_template(&template)?;
            let summary = Workspace::init(&root, template)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Command::Status { root } => {
            let workspace = Workspace::open(&root, Template::default_template())?;
            let status = workspace.status()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "root": status.root,
                    "template": status.template,
                    "head_snapshot_id": status.head_snapshot_id,
                    "dirty_paths": status.dirty_paths,
                    "unsafe_state": status.unsafe_state,
                    "recovery_pending": status.recovery_pending,
                    "page_count": status.page_count,
                }))?
            );
        }
        Command::Snapshot { root, message } => {
            let workspace = Workspace::open(&root, Template::default_template())?;
            let snapshot = workspace.snapshot(&message)?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        Command::History { root, limit } => {
            let workspace = Workspace::open(&root, Template::default_template())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&workspace.history(limit)?)?
            );
        }
        Command::Restore {
            root,
            snapshot,
            yes,
        } => {
            if !yes {
                anyhow::bail!("restore requires --yes");
            }
            let workspace = Workspace::open(&root, Template::default_template())?;
            workspace.restore(&snapshot)?;
            println!("restored to {snapshot}");
        }
    }
    Ok(())
}
