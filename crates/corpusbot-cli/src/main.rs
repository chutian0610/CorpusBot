use clap::{Parser, Subcommand};
use corpusbot_agent::{QueryContextPage, RigLlmClient, SourceAgent, provider_config};
use corpusbot_core::{Template, VERSION};
use corpusbot_ingest::Ingestor;
use corpusbot_lint::engine::run_lint;
use corpusbot_search::SearchIndex;
use corpusbot_store::{GitIdentity, Workspace};
use serde::{Deserialize, Serialize};
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
    /// Check workspace health without modifying files.
    Lint {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Run the fixed citation-quality evaluation against a workspace.
    Evaluate {
        #[arg(long)]
        root: std::path::PathBuf,
        #[arg(long)]
        questions: std::path::PathBuf,
        #[arg(long, default_value_t = 8)]
        limit: usize,
        #[arg(long, default_value_t = 0.8)]
        min_pass_rate: f64,
        #[arg(long)]
        output: Option<std::path::PathBuf>,
    },
}

fn parse_template(value: &str) -> anyhow::Result<Template> {
    match value {
        "generic" => Ok(Template::Generic),
        "research" => Ok(Template::Research),
        other => Err(anyhow::anyhow!("unknown template: {other}")),
    }
}

fn workspace(root: &std::path::Path) -> anyhow::Result<Workspace> {
    let identity = corpusbot_agent::git_identity()?;
    let identity = identity.map(|(name, email)| GitIdentity { name, email });
    Workspace::open_with_identity(root, Template::default_template(), identity).map_err(Into::into)
}

#[derive(Debug, Deserialize)]
struct EvaluationCase {
    id: String,
    question: String,
}

#[derive(Debug, Serialize)]
struct EvaluationResult {
    id: String,
    question: String,
    passed: bool,
    error: Option<String>,
    answer: Option<String>,
    insufficient_evidence: Option<bool>,
    citation_count: Option<usize>,
    citations: Vec<corpusbot_agent::Citation>,
    warnings: Vec<String>,
    revision_manifest_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct EvaluationReport {
    workspace: String,
    manifest_id: String,
    total: usize,
    passed: usize,
    pass_rate: f64,
    results: Vec<EvaluationResult>,
}

fn query_context(
    root: &std::path::Path,
    question: &str,
    limit: usize,
) -> anyhow::Result<(String, Vec<QueryContextPage>)> {
    let workspace = workspace(root)?;
    if workspace.status()?.recovery_pending {
        anyhow::bail!("workspace has pending recovery");
    }
    let manifest = workspace.revision_manifest()?;
    let index = SearchIndex::new(workspace.paths().search_index.clone());

    let context = index
        .search(question, limit)?
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
    Ok((manifest.manifest_id().to_owned(), context))
}

fn write_json(path: &std::path::Path, value: &impl Serialize) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

async fn evaluate_workspace(
    root: &std::path::Path,
    cases: &[EvaluationCase],
    limit: usize,
) -> anyhow::Result<EvaluationReport> {
    let client = RigLlmClient::new(provider_config()?)?;
    let agent = SourceAgent::new(client);
    let mut results = Vec::with_capacity(cases.len());

    for case in cases {
        let outcome = async {
            let (manifest_id, context) = query_context(root, &case.question, limit)?;
            let answer = agent
                .answer_question_audited(
                    root,
                    &corpusbot_agent::query_run_id(),
                    &manifest_id,
                    &case.question,
                    &context,
                )
                .await?;
            anyhow::Ok(answer)
        }
        .await;

        let result = match outcome {
            Ok(answer) => {
                let passed = !answer.insufficient_evidence && !answer.citations.is_empty();
                EvaluationResult {
                    id: case.id.clone(),
                    question: case.question.clone(),
                    passed,
                    error: None,
                    answer: Some(answer.answer),
                    insufficient_evidence: Some(answer.insufficient_evidence),
                    citation_count: Some(answer.citations.len()),
                    citations: answer.citations,
                    warnings: answer.warnings,
                    revision_manifest_id: Some(answer.revision_manifest_id),
                }
            }
            Err(error) => EvaluationResult {
                id: case.id.clone(),
                question: case.question.clone(),
                passed: false,
                error: Some(error.to_string()),
                answer: None,
                insufficient_evidence: None,
                citation_count: None,
                citations: Vec::new(),
                warnings: Vec::new(),
                revision_manifest_id: None,
            },
        };
        results.push(result);
    }

    let manifest_id = workspace(root)?
        .revision_manifest()?
        .manifest_id()
        .to_owned();
    let passed = results.iter().filter(|result| result.passed).count();
    let total = results.len();
    let pass_rate = if total == 0 {
        0.0
    } else {
        passed as f64 / total as f64
    };
    Ok(EvaluationReport {
        workspace: root.display().to_string(),
        manifest_id,
        total,
        passed,
        pass_rate,
        results,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { root, template } => {
            let template = parse_template(&template)?;
            let identity = corpusbot_agent::git_identity()?;
            let identity = identity.map(|(name, email)| GitIdentity { name, email });
            let summary = Workspace::init_with_identity(&root, template, identity)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Command::Status { root } => {
            let workspace = workspace(&root)?;
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
            let workspace = workspace(&root)?;
            let snapshot = workspace.snapshot(&message)?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        Command::History { root, limit } => {
            let workspace = workspace(&root)?;
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
            let workspace = workspace(&root)?;
            workspace.restore(&snapshot)?;
            println!("restored to {snapshot}");
        }
        Command::Ingest { root, file } => {
            let workspace = workspace(&root)?;
            let client = RigLlmClient::new(provider_config()?)?;
            let result = Ingestor::new(client).ingest_file(&workspace, &file).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Query {
            root,
            question,
            limit,
        } => {
            let (manifest_id, context) = query_context(&root, &question, limit)?;
            let client = RigLlmClient::new(provider_config()?)?;
            let agent = SourceAgent::new(client);
            let answer = agent
                .answer_question_audited(
                    root.as_path(),
                    &corpusbot_agent::query_run_id(),
                    &manifest_id,
                    &question,
                    &context,
                )
                .await?;
            println!("{}", serde_json::to_string_pretty(&answer)?);
        }
        Command::Lint { root, format } => {
            let workspace = workspace(&root)?;
            if workspace.status()?.recovery_pending {
                anyhow::bail!("workspace has pending recovery");
            }
            let manifest = workspace.revision_manifest()?;
            let report = run_lint(&root, workspace.template(), manifest.manifest_id())?;
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&report)?),
                "table" => {
                    println!(
                        "pages={} errors={} warnings={}",
                        report.summary.pages, report.summary.errors, report.summary.warnings
                    );
                    for issue in &report.issues {
                        println!(
                            "{:<8} {:<24} {}",
                            issue.severity.as_str().to_uppercase(),
                            issue.code,
                            issue.message
                        );
                        println!("  path: {}", issue.path);
                        println!("  hint: {}", issue.fix_hint);
                    }
                }
                other => anyhow::bail!("unknown lint format: {other}"),
            }
        }
        Command::Evaluate {
            root,
            questions,
            limit,
            min_pass_rate,
            output,
        } => {
            let raw = std::fs::read_to_string(&questions)?;
            let cases: Vec<EvaluationCase> = serde_json::from_str(&raw)?;
            anyhow::ensure!(!cases.is_empty(), "evaluation has no questions");
            let report = evaluate_workspace(&root, &cases, limit).await?;
            let passed = report.pass_rate >= min_pass_rate;
            if let Some(output) = output {
                write_json(&output, &report)?;
            }
            println!("{}", serde_json::to_string_pretty(&report)?);
            anyhow::ensure!(
                passed,
                "evaluation pass rate {:.1}% is below {:.1}%",
                report.pass_rate * 100.0,
                min_pass_rate * 100.0
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvp_evaluation_fixture_has_ten_unique_cases() -> anyhow::Result<()> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/mvp-eval/questions.json");
        let cases: Vec<EvaluationCase> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        assert_eq!(cases.len(), 10);
        let ids = cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids.len(),
            ids.iter().collect::<std::collections::HashSet<_>>().len()
        );
        assert!(cases.iter().all(|case| !case.question.trim().is_empty()));
        Ok(())
    }
}
