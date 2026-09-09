use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use corpusbot_agent::{DraftPlan, LlmClient, SourceAgent, SourceAnalysis};
use corpusbot_core::{
    Frontmatter, IsoDate, PageIdentity, PageType, ResourceId, ResourceRevision, Revision,
    SourceRef, WikiDoc, Wikilink,
};
use corpusbot_store::{PageFile, SourceRow, Workspace};
use serde::Serialize;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::error::{IngestError, Result};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum IngestResult {
    Committed {
        run_id: String,
        source_id: String,
        source_version_id: String,
        source_page: String,
        created_paths: Vec<String>,
        updated_paths: Vec<String>,
        snapshot_id: String,
        manifest_id: String,
    },
    Duplicate {
        source_id: String,
        source_version_id: String,
    },
}

pub struct Ingestor<C> {
    agent: SourceAgent<C>,
}

impl<C> Ingestor<C>
where
    C: LlmClient,
{
    pub fn new(client: C) -> Self {
        Self {
            agent: SourceAgent::new(client),
        }
    }

    pub async fn ingest_content(
        &self,
        workspace: &Workspace,
        original_name: &str,
        markdown: &str,
    ) -> Result<IngestResult> {
        let extension = Path::new(original_name)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("md");
        let mut temporary = tempfile::Builder::new()
            .prefix("corpusbot-import-")
            .suffix(&format!(".{extension}"))
            .tempfile()?;
        std::io::Write::write_all(&mut temporary, markdown.as_bytes())?;
        temporary.flush()?;
        let result = self.ingest_file(workspace, temporary.path()).await?;
        temporary.close()?;
        Ok(result)
    }

    pub async fn ingest_file(
        &self,
        workspace: &Workspace,
        source_path: &Path,
    ) -> Result<IngestResult> {
        let status = workspace.status()?;
        if status.recovery_pending {
            return Err(corpusbot_store::StoreError::RecoveryPending.into());
        }
        if let Some(state) = status.unsafe_state {
            return Err(IngestError::Vcs(corpusbot_vcs::VcsError::UnsafeState(
                state,
            )));
        }
        if !status.dirty_paths.is_empty() {
            return Err(IngestError::WorkspaceDirty {
                paths: status.dirty_paths,
            });
        }

        let content = std::fs::read(source_path)?;
        let markdown =
            String::from_utf8(content).map_err(|_| IngestError::InvalidSourceEncoding)?;
        let sha256 = hex(markdown.as_bytes());
        if let Some(source) = workspace.source_by_sha(&sha256)? {
            return Ok(IngestResult::Duplicate {
                source_id: source.source_id,
                source_version_id: source.source_version_id,
            });
        }

        let original_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && !matches!(*name, "." | ".."))
            .ok_or(IngestError::InvalidSourceName)?;
        let now = OffsetDateTime::now_utc();
        let source_id = format!("src_{}", &sha256[..24]);
        let source_version_id = format!("ver_{}", &sha256[..24]);
        let raw_path = format!("raw/{sha256}/{original_name}");
        let baseline = workspace.revision_manifest()?;
        let source_record = corpusbot_core::SourceRecord::new(
            &source_id,
            &source_version_id,
            original_name,
            &sha256,
            markdown.len() as u64,
            now,
        )?;

        let analysis = self
            .analyze_with_retry(&source_version_id, &markdown)
            .await?;
        let existing = existing_pages(workspace)?;
        let related = related_pages(&existing);
        let mut plan = self
            .agent
            .generate_drafts(
                workspace.template().as_str(),
                &analysis.title,
                &markdown,
                &analysis,
                &related,
            )
            .await?;
        validate_plan(&plan, 1)?;
        if !valid_plan_identities(workspace, &analysis, &plan) {
            plan = self
                .agent
                .generate_drafts(
                    workspace.template().as_str(),
                    &analysis.title,
                    &markdown,
                    &analysis,
                    &related,
                )
                .await?;
            validate_plan(&plan, 2)?;
        }

        let source_page = format!("wiki/sources/{source_version_id}.md");
        let source_ref = SourceRef::new(&source_version_id, &analysis.title)?;
        let source_link = Wikilink::new(&source_page)?;
        let source_title = format!("{} ({})", analysis.title, &source_version_id[4..16]);
        let source_body = format!(
            "# {}\n\n{}\n\n## Captured source\n\nOriginal file: `{original_name}`\n",
            analysis.title, analysis.summary
        );
        let source_markdown = render_page(
            workspace,
            &source_page,
            PageType::Source,
            &source_title,
            now,
            vec![],
            vec![],
            vec![],
            vec![source_ref.clone()],
            &source_body,
        )?;

        let mut files = BTreeMap::new();
        let mut pages = Vec::new();
        let mut touched = vec![ResourceRevision::absent(ResourceId::new(&raw_path)?)];
        let mut created = vec![source_page.clone()];
        let mut updated = Vec::new();
        files.insert(source_page.clone(), source_markdown.clone().into_bytes());
        pages.push(PageFile {
            path: source_page.clone(),
            markdown: source_markdown,
            title: source_title.clone(),
            page_type: PageType::Source.as_str().to_owned(),
            updated_at: iso_date(now),
        });

        for entity in &plan.entities {
            let (path, markdown, is_new) = entity_page(
                workspace,
                &existing,
                &entity.name,
                &entity.aliases,
                &entity.summary,
                &source_ref,
                &source_link,
                now,
            )?;
            let revision = if is_new {
                Revision::absent()
            } else {
                baseline.expected(&ResourceId::page(corpusbot_core::WikiPath::parse(&path)?))
            };
            touched.push(ResourceRevision::new(
                ResourceId::page(corpusbot_core::WikiPath::parse(&path)?),
                revision,
            ));
            files.insert(path.clone(), markdown.clone().into_bytes());
            pages.push(PageFile {
                path: path.clone(),
                markdown,
                title: entity.name.clone(),
                page_type: PageType::Entity.as_str().to_owned(),
                updated_at: iso_date(now),
            });
            if is_new {
                created.push(path);
            } else {
                updated.push(path);
            }
        }

        for concept in &plan.concepts {
            let (path, markdown, is_new) = concept_page(
                workspace,
                &existing,
                &concept.name,
                &concept.definition,
                &source_ref,
                &source_link,
                now,
            )?;
            let revision = if is_new {
                Revision::absent()
            } else {
                baseline.expected(&ResourceId::page(corpusbot_core::WikiPath::parse(&path)?))
            };
            touched.push(ResourceRevision::new(
                ResourceId::page(corpusbot_core::WikiPath::parse(&path)?),
                revision,
            ));
            files.insert(path.clone(), markdown.clone().into_bytes());
            pages.push(PageFile {
                path: path.clone(),
                markdown,
                title: concept.name.clone(),
                page_type: PageType::Concept.as_str().to_owned(),
                updated_at: iso_date(now),
            });
            if is_new {
                created.push(path);
            } else {
                updated.push(path);
            }
        }

        let index = render_index(&existing, &source_title, &source_page);
        let log = append_log(workspace, &analysis.title, &source_version_id)?;
        let index_resource = ResourceId::index();
        let log_resource = ResourceId::log();
        let index_expected = baseline.expected(&index_resource);
        let log_expected = baseline.expected(&log_resource);
        touched.push(ResourceRevision::new(index_resource, index_expected));
        touched.push(ResourceRevision::new(log_resource, log_expected));
        files.insert("wiki/index.md".to_owned(), index.into_bytes());
        files.insert("wiki/log.md".to_owned(), log.into_bytes());

        let request = corpusbot_store::IngestCommitRequest {
            run_id: format!("ingest_{}_{:x}", &sha256[..12], now.unix_timestamp_nanos()),
            message: format!("ingest {}", analysis.title),
            source: Some(source_row(&source_record)?),
            files: files.into_iter().collect(),
            pages,
            touched,
            baseline_manifest: baseline,
        };
        workspace.begin_ingest(&request)?;
        let committed = workspace.commit_ingest(&request)?;
        workspace.reconcile_pending_ingest()?;
        workspace.rebuild_search_index()?;

        Ok(IngestResult::Committed {
            run_id: committed.run_id,
            source_id,
            source_version_id,
            source_page,
            created_paths: created,
            updated_paths: updated,
            snapshot_id: committed.snapshot_id,
            manifest_id: committed.manifest_id,
        })
    }

    async fn analyze_with_retry(
        &self,
        source_hint: &str,
        markdown: &str,
    ) -> Result<SourceAnalysis> {
        let mut last_error = None;
        for _ in 0..crate::MAX_ANALYSIS_ATTEMPTS {
            match self.agent.analyze_source(source_hint, markdown).await {
                Ok(analysis) => return Ok(analysis),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.map_or_else(
            || IngestError::Agent(corpusbot_agent::AgentError::EmptyResponse),
            IngestError::Agent,
        ))
    }
}

fn existing_pages(workspace: &Workspace) -> Result<BTreeMap<String, ExistingWikiPage>> {
    let mut pages = BTreeMap::new();
    for entry in walkdir::WalkDir::new(workspace.paths().wiki_dir.clone()).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .strip_prefix(workspace.paths().root.clone())
            .map_err(|error| IngestError::Core(corpusbot_core::CoreError::Path(error.to_string())))?
            .to_string_lossy()
            .replace('\\', "/");
        if matches!(path.as_str(), "wiki/index.md" | "wiki/log.md") {
            continue;
        }
        let Ok(wiki_path) = corpusbot_core::WikiPath::parse(&path) else {
            continue;
        };
        let markdown = std::fs::read_to_string(entry.path())?;
        if let Ok((document, _)) = WikiDoc::parse_markdown(wiki_path, &markdown) {
            let identity = document.frontmatter().identity(workspace.template())?;
            pages.insert(identity.key(), ExistingWikiPage { path, document });
        }
    }
    Ok(pages)
}

fn related_pages(existing: &BTreeMap<String, ExistingWikiPage>) -> Vec<(String, String, String)> {
    existing
        .values()
        .take(8)
        .map(|page| {
            (
                page.path.clone(),
                page.document.frontmatter().title().to_owned(),
                page.document.body().chars().take(600).collect(),
            )
        })
        .collect()
}

fn valid_plan_identities(
    workspace: &Workspace,
    analysis: &SourceAnalysis,
    plan: &DraftPlan,
) -> bool {
    let entities_valid = analysis.entities.iter().all(|entity| {
        PageIdentity::new(workspace.template(), PageType::Entity, &entity.name).is_ok()
    }) && plan.entities.iter().all(|entity| {
        PageIdentity::new(workspace.template(), PageType::Entity, &entity.name).is_ok()
    });
    let concepts_valid = analysis.concepts.iter().all(|concept| {
        PageIdentity::new(workspace.template(), PageType::Concept, &concept.name).is_ok()
    }) && plan.concepts.iter().all(|concept| {
        PageIdentity::new(workspace.template(), PageType::Concept, &concept.name).is_ok()
    });
    entities_valid && concepts_valid
}

fn validate_plan(plan: &DraftPlan, attempt: u32) -> Result<()> {
    if plan.source_summary.trim().is_empty() {
        return Err(IngestError::SelfAudit(format!(
            "attempt {attempt}: source summary is empty"
        )));
    }
    if plan.source_summary.chars().count() > 4000 {
        return Err(IngestError::SelfAudit(format!(
            "attempt {attempt}: source summary is too long"
        )));
    }
    for entity in &plan.entities {
        if entity.name.trim().is_empty() || entity.summary.trim().chars().count() < 8 {
            return Err(IngestError::SelfAudit(format!(
                "attempt {attempt}: invalid entity {}",
                entity.name
            )));
        }
    }
    for concept in &plan.concepts {
        if concept.name.trim().is_empty() || concept.definition.trim().chars().count() < 8 {
            return Err(IngestError::SelfAudit(format!(
                "attempt {attempt}: invalid concept {}",
                concept.name
            )));
        }
    }
    Ok(())
}

struct ExistingWikiPage {
    path: String,
    document: WikiDoc,
}

fn find_existing<'a>(
    workspace: &Workspace,
    existing: &'a BTreeMap<String, ExistingWikiPage>,
    page_type: PageType,
    name: &str,
) -> Option<&'a ExistingWikiPage> {
    let key = PageIdentity::new(workspace.template(), page_type, name)
        .ok()?
        .key();
    existing.get(&key)
}

fn unique_path(workspace: &Workspace, page_type: PageType, name: &str) -> String {
    let identity =
        PageIdentity::new(workspace.template(), page_type, name).expect("validated identity");
    let slug = slugify(identity.canonical_name());
    let base = format!("{}/{slug}.md", page_type.default_directory());
    if !workspace.paths().root.join(&base).exists() {
        return base;
    }
    let digest = Sha256::digest(identity.key().as_bytes());
    format!(
        "{}/{}-{:.8}.md",
        page_type.default_directory(),
        slug,
        format!("{digest:x}")
    )
}

fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_whitespace() {
                '-'
            } else {
                character
            }
        })
        .filter(|character| character.is_alphanumeric() || matches!(*character, '-' | '_'))
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if slug.is_empty() {
        "page".to_owned()
    } else {
        slug
    }
}

#[allow(clippy::too_many_arguments)]
fn render_page(
    workspace: &Workspace,
    path: &str,
    page_type: PageType,
    title: &str,
    now: OffsetDateTime,
    tags: Vec<String>,
    aliases: Vec<String>,
    related: Vec<Wikilink>,
    sources: Vec<SourceRef>,
    body: &str,
) -> Result<String> {
    let date = IsoDate::parse(iso_date(now))?;
    let frontmatter = Frontmatter::new(
        page_type, title, date, date, tags, aliases, related, sources,
    )?;
    let wiki_path = corpusbot_core::WikiPath::parse(path)?;
    workspace
        .template()
        .definition()
        .validate_doc(&wiki_path, &frontmatter)?;
    Ok(format!(
        "---\n{}---\n\n{body}\n",
        serde_yaml::to_string(&frontmatter)?
    ))
}

#[allow(clippy::too_many_arguments)]
fn entity_page(
    workspace: &Workspace,
    existing: &BTreeMap<String, ExistingWikiPage>,
    name: &str,
    aliases: &[String],
    summary: &str,
    source_ref: &SourceRef,
    source_link: &Wikilink,
    now: OffsetDateTime,
) -> Result<(String, String, bool)> {
    let Some(page) = find_existing(workspace, existing, PageType::Entity, name) else {
        let path = unique_path(workspace, PageType::Entity, name);
        let body = format!(
            "# {name}\n\n{summary}\n\n## From {}\n\nSource: {}\n",
            source_ref.title(),
            source_link.render()
        );
        let markdown = render_page(
            workspace,
            &path,
            PageType::Entity,
            name,
            now,
            vec![],
            aliases.to_vec(),
            vec![source_link.clone()],
            vec![source_ref.clone()],
            &body,
        )?;
        return Ok((path, markdown, true));
    };

    let old = page.document.clone();
    let old_frontmatter = old.frontmatter();
    let mut tags = old_frontmatter.tags().to_vec();
    let aliases = old_frontmatter.aliases().to_vec();
    let mut normalized_aliases = Vec::with_capacity(aliases.len());
    let mut related = old_frontmatter.related().to_vec();
    let mut sources = old_frontmatter.sources().to_vec();
    push_unique(&mut tags, "ingested".to_owned());
    for alias in aliases {
        push_unique(&mut normalized_aliases, PageIdentity::normalize(alias)?);
    }
    push_wikilink(&mut related, source_link);
    push_source(&mut sources, source_ref.clone())?;
    let date = IsoDate::parse(iso_date(now))?;
    let frontmatter = Frontmatter::new(
        PageType::Entity,
        old_frontmatter.title(),
        old_frontmatter.created(),
        date,
        tags,
        normalized_aliases,
        related,
        sources,
    )?;
    let markdown = format!(
        "---\n{}---\n\n{}\n\n## From {}\n\n{}\n\nSource: {}\n",
        serde_yaml::to_string(&frontmatter)?,
        old.body(),
        source_ref.title(),
        summary,
        source_link.render()
    );
    Ok((page.path.clone(), markdown, false))
}

#[allow(clippy::too_many_arguments)]
fn concept_page(
    workspace: &Workspace,
    existing: &BTreeMap<String, ExistingWikiPage>,
    name: &str,
    definition: &str,
    source_ref: &SourceRef,
    source_link: &Wikilink,
    now: OffsetDateTime,
) -> Result<(String, String, bool)> {
    let Some(page) = find_existing(workspace, existing, PageType::Concept, name) else {
        let path = unique_path(workspace, PageType::Concept, name);
        let body = format!(
            "# {name}\n\n{definition}\n\n## From {}\n\nSource: {}\n",
            source_ref.title(),
            source_link.render()
        );
        let markdown = render_page(
            workspace,
            &path,
            PageType::Concept,
            name,
            now,
            vec![],
            vec![],
            vec![source_link.clone()],
            vec![source_ref.clone()],
            &body,
        )?;
        return Ok((path, markdown, true));
    };

    let old = page.document.clone();
    let old_frontmatter = old.frontmatter();
    let mut tags = old_frontmatter.tags().to_vec();
    let mut related = old_frontmatter.related().to_vec();
    let mut sources = old_frontmatter.sources().to_vec();
    push_unique(&mut tags, "ingested".to_owned());
    push_wikilink(&mut related, source_link);
    push_source(&mut sources, source_ref.clone())?;
    let date = IsoDate::parse(iso_date(now))?;
    let frontmatter = Frontmatter::new(
        PageType::Concept,
        old_frontmatter.title(),
        old_frontmatter.created(),
        date,
        tags,
        old_frontmatter.aliases().to_vec(),
        related,
        sources,
    )?;
    let markdown = format!(
        "---\n{}---\n\n{}\n\n## From {}\n\n{}\n\nSource: {}\n",
        serde_yaml::to_string(&frontmatter)?,
        old.body(),
        source_ref.title(),
        definition,
        source_link.render()
    );
    Ok((page.path.clone(), markdown, false))
}

fn render_index(
    existing: &BTreeMap<String, ExistingWikiPage>,
    source_title: &str,
    source_page: &str,
) -> String {
    let mut lines = vec!["# Index".to_owned(), String::new()];
    lines.push(format!("- [[{source_page}|{source_title}]]"));
    let mut pages = existing
        .values()
        .map(|page| {
            (
                page.path.clone(),
                page.document.frontmatter().title().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    pages.sort_by(|left, right| {
        left.1
            .to_lowercase()
            .cmp(&right.1.to_lowercase())
            .then(left.0.cmp(&right.0))
    });
    for (path, title) in pages {
        lines.push(format!("- [[{path}|{title}]]"));
    }
    lines.join("\n") + "\n"
}

fn append_log(workspace: &Workspace, title: &str, source_version_id: &str) -> Result<String> {
    let path = workspace.paths().root.join("wiki/log.md");
    let current = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let date = iso_date(OffsetDateTime::now_utc());
    Ok(format!(
        "{current}\n## [{date}] ingest | {title}\n\n- source version: {source_version_id}\n"
    ))
}

fn source_row(value: &corpusbot_core::SourceRecord) -> Result<SourceRow> {
    Ok(SourceRow {
        source_id: value.source_id().to_owned(),
        source_version_id: value.source_version_id().to_owned(),
        sha256: value.sha256().to_owned(),
        original_name: value.original_name().to_owned(),
        size: value.size(),
        imported_at: value
            .imported_at()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|error| {
                IngestError::Core(corpusbot_core::CoreError::Frontmatter(error.to_string()))
            })?,
    })
}

fn hex(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn push_wikilink(values: &mut Vec<Wikilink>, value: &Wikilink) {
    if !values.iter().any(|item| item.target() == value.target()) {
        values.push(value.clone());
    }
}

fn push_source(values: &mut Vec<SourceRef>, value: SourceRef) -> Result<()> {
    if !values
        .iter()
        .any(|item| item.source_version_id() == value.source_version_id())
    {
        values.push(value);
    }
    Ok(())
}

fn iso_date(value: OffsetDateTime) -> String {
    value.date().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use corpusbot_agent::FakeLlmClient;
    use corpusbot_core::Template;

    const SOURCE: &str = r#"# Raft

Raft elects a leader before replicating log entries. A candidate needs a majority."#;

    const ANALYSIS: &str = r#"{
      "title": "Raft",
      "summary": "Raft is a leader-based consensus algorithm.",
      "entities": [
        {"name": "Raft", "aliases": ["Raft consensus"], "summary": "A leader-based consensus algorithm."}
      ],
      "concepts": [
        {"name": "Leader Election", "definition": "The process of selecting a coordinator."}
      ]
    }"#;

    const DRAFTS: &str = r#"{
      "source_summary": "Raft is a leader-based consensus algorithm.",
      "entities": [
        {"name": "Raft", "aliases": ["Raft consensus"], "summary": "A leader-based consensus algorithm."}
      ],
      "concepts": [
        {"name": "Leader Election", "definition": "The process of selecting a coordinator."}
      ]
    }"#;

    #[tokio::test]
    async fn ingests_a_file_once() -> Result<()> {
        let root = tempfile::tempdir()?;
        corpusbot_store::Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let source = root.path().join("raft.md");
        std::fs::write(&source, SOURCE)?;
        let ingestor = Ingestor::new(FakeLlmClient::new([ANALYSIS, DRAFTS]));

        let result = ingestor.ingest_file(&workspace, &source).await?;
        let IngestResult::Committed {
            source_page,
            snapshot_id,
            created_paths,
            ..
        } = result
        else {
            panic!("expected committed result");
        };
        assert!(source_page.starts_with("wiki/sources/ver_"));
        assert_eq!(created_paths.len(), 3);
        assert!(root.path().join(&source_page).exists());
        assert!(
            root.path()
                .join("wiki/concepts/leader-election.md")
                .exists()
        );
        assert_eq!(snapshot_id.len(), 40);

        let duplicate = ingestor.ingest_file(&workspace, &source).await?;
        assert!(matches!(duplicate, IngestResult::Duplicate { .. }));
        Ok(())
    }

    #[tokio::test]
    async fn dirty_workspace_blocks_before_llm_calls() -> Result<()> {
        let root = tempfile::tempdir()?;
        corpusbot_store::Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        std::fs::write(root.path().join("wiki/manual.md"), "# Manual\n")?;
        let source = root.path().join("raft.md");
        std::fs::write(&source, SOURCE)?;
        let ingestor = Ingestor::new(FakeLlmClient::new(Vec::<String>::new()));

        assert!(matches!(
            ingestor.ingest_file(&workspace, &source).await,
            Err(IngestError::WorkspaceDirty { .. })
        ));
        std::fs::remove_file(source)?;
        Ok(())
    }

    #[tokio::test]
    async fn llm_failure_does_not_mutate_tracked_workspace() -> Result<()> {
        let root = tempfile::tempdir()?;
        corpusbot_store::Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let source = root.path().join("raft.md");
        std::fs::write(&source, SOURCE)?;
        let ingestor = Ingestor::new(FakeLlmClient::new(Vec::<String>::new()));

        assert!(matches!(
            ingestor.ingest_file(&workspace, &source).await,
            Err(IngestError::Agent(_))
        ));
        let raw_file = root
            .path()
            .join("raw")
            .join(hex(SOURCE.as_bytes()))
            .join("raft.md");
        assert!(!raw_file.exists());
        let status = workspace.status()?;
        assert!(status.dirty_paths.is_empty());
        assert!(!status.recovery_pending);
        Ok(())
    }
}
