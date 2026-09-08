use clap::{Parser, Subcommand};
use corpusbot_agent::{ProviderConfig, QueryContextPage, RigLlmClient, SourceAgent};
use corpusbot_core::{Template, VERSION};
use corpusbot_ingest::Ingestor;
use corpusbot_search::SearchIndex;
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
    /// Ingest one Markdown source.
    Ingest {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long)]
        file: std::path::PathBuf,
    },
    /// Answer a question with citations.
    Query {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long)]
        question: String,
        #[arg(long, default_value_t = 8)]
        limit: usize,
    },
}

fn parse_template(value: &str) -> anyhow::Result<Template> {
    match value {
        "generic" => Ok(Template::Generic),
        "research" => Ok(Template::Research),
        other => Err(anyhow::anyhow!("unknown template: {other}")),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        Command::Ingest { root, file } => {
            let workspace = Workspace::open(&root, Template::default_template())?;
            let client = RigLlmClient::new(ProviderConfig::load()?)?;
            let result = Ingestor::new(client).ingest_file(&workspace, &file).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Query {
            root,
            question,
            limit,
        } => {
            let workspace = Workspace::open(&root, Template::default_template())?;
            if workspace.status()?.recovery_pending {
                anyhow::bail!("workspace has pending recovery");
            }
            let manifest = workspace.revision_manifest()?;
            let index = SearchIndex::new(workspace.paths().search_index.clone());
            let client = RigLlmClient::new(ProviderConfig::load()?)?;
            let agent = SourceAgent::new(client);

            let context = index
                .search(&question, limit)?
                .into_iter()
                .filter_map(|hit| {
                    let resource = corpusbot_core::ResourceId::new(&hit.path).ok()?;
                    let revision = manifest.expected(&resource);
                    if matches!(revision, corpusbot_core::Revision::Absent) {
                        return None;
                    }
                    let markdown = workspace.read_page(&hit.path).ok()?;
                    Some(QueryContextPage {
                        path: hit.path,
                        title: hit.title,
                        page_type: hit.page_type,
                        revision,
                        revision_manifest_id: manifest.manifest_id().to_owned(),
                        markdown,
                    })
                })
                .collect::<Vec<_>>();

            let mut remaining_context_chars = 24_000usize;
            let context = context
                .into_iter()
                .map(|mut page| {
                    let budget = remaining_context_chars.min(page.markdown.chars().count());
                    page.markdown = page.markdown.chars().take(budget).collect();
                    remaining_context_chars -= budget;
                    page
                })
                .collect::<Vec<_>>();

            let answer = agent.answer_question(&question, &context).await?;
            println!("{}", serde_json::to_string_pretty(&answer)?);
        }
    }
    Ok(())
}
