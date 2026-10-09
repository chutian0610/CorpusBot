use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use corpusbot_agent::{
    ConceptDraft, DraftPlan, DraftSection, EntityDraft, LlmClient, SourceAgent, validate_draft_plan,
};
use corpusbot_core::{
    Frontmatter, IsoDate, PageIdentity, PageType, RawReference, ResourceId, ResourceRevision,
    Revision, SourceRef, WikiDoc, Wikilink, split_raw_markdown,
};
use corpusbot_store::{PageFile, SourceRow, Workspace};
use serde::Serialize;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::error::{IngestError, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestStage {
    Analyze,
    Draft,
    Commit,
}

impl IngestStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Analyze => "analyze",
            Self::Draft => "draft",
            Self::Commit => "commit",
        }
    }
}

#[derive(Clone)]
pub struct IngestProgressUpdate {
    pub stage: IngestStage,
    pub run_id: String,
}

pub type IngestProgressCallback = Arc<dyn Fn(IngestProgressUpdate) + Send + Sync>;

#[derive(Clone, Debug, Serialize)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "status"
)]
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
    progress: Option<IngestProgressCallback>,
}

impl<C> Ingestor<C>
where
    C: LlmClient + 'static,
{
    pub fn new(client: C) -> Self {
        Self {
            agent: SourceAgent::new(client),
            progress: None,
        }
    }

    pub fn with_max_draft_tokens(mut self, max_draft_tokens: u64) -> Self {
        self.agent = self.agent.with_max_draft_tokens(max_draft_tokens);
        self
    }

    pub fn with_progress(mut self, progress: IngestProgressCallback) -> Self {
        self.progress = Some(progress);
        self
    }

    fn report_progress(&self, stage: IngestStage, run_id: &str) {
        if let Some(progress) = &self.progress {
            progress(IngestProgressUpdate {
                stage,
                run_id: run_id.to_owned(),
            });
        }
    }

    pub async fn ingest_content(
        &self,
        workspace: &Workspace,
        original_name: &str,
        markdown: &str,
    ) -> Result<IngestResult> {
        let original_name = validated_source_name(original_name)?;
        self.ingest_bytes(workspace, original_name, markdown).await
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
        let original_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(IngestError::InvalidSourceName)?;
        self.ingest_bytes(workspace, validated_source_name(original_name)?, &markdown)
            .await
    }

    async fn ingest_bytes(
        &self,
        workspace: &Workspace,
        original_name: &str,
        markdown: &str,
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

        let sha256 = hex(markdown.as_bytes());
        if let Some(source) = workspace.source_by_sha(&sha256)? {
            return Ok(IngestResult::Duplicate {
                source_id: source.source_id,
                source_version_id: source.source_version_id,
            });
        }

        let now = OffsetDateTime::now_utc();
        let source_id = format!("src_{}", &sha256[..24]);
        let source_version_id = format!("ver_{}", &sha256[..24]);
        let run_id = format!("ingest_{}_{:x}", &sha256[..12], now.unix_timestamp_nanos());
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

        // Keep the raw file unchanged, but don't let its frontmatter leak into
        // LLM analysis and generation prompts.
        let source_markdown = split_raw_markdown(markdown).body;

        self.report_progress(IngestStage::Analyze, &run_id);
        let analysis = self
            .agent
            .analyze_source_audited(
                workspace.paths().root.as_path(),
                &run_id,
                baseline.manifest_id(),
                &source_version_id,
                &source_markdown,
            )
            .await?;
        let existing = existing_pages(workspace)?;
        let related = related_pages(&existing);
        self.report_progress(IngestStage::Draft, &run_id);
        let plan = self
            .agent
            .generate_drafts_audited(
                workspace.paths().root.as_path(),
                &run_id,
                baseline.manifest_id(),
                workspace.template(),
                &analysis.title,
                &source_markdown,
                &analysis,
                &related,
            )
            .await?;
        validate_plan(&plan, workspace.template(), 1)?;
        self.report_progress(IngestStage::Commit, &run_id);

        let source_page = format!("wiki/sources/{source_version_id}.md");
        let source_ref = SourceRef::new(&source_version_id, &analysis.title)?;
        let raw_reference =
            RawReference::new(&raw_path, original_name, &sha256, markdown.len() as u64)?;
        let source_link = Wikilink::new(&source_page)?;
        let source_title = format!("{} ({})", analysis.title, &source_version_id[4..16]);
        let entities = unique_entities(workspace.template(), &plan)?;
        let concepts = unique_concepts(workspace.template(), &plan)?;
        let mut entity_pages = Vec::new();
        for entity in &entities {
            let path = existing
                .get(
                    &PageIdentity::new(workspace.template(), PageType::Entity, &entity.name)?.key(),
                )
                .map(|page| page.path.clone())
                .unwrap_or_else(|| unique_path(workspace, PageType::Entity, &entity.name));
            entity_pages.push((path, entity.name.clone()));
        }
        let mut concept_pages = Vec::new();
        for concept in &concepts {
            let path = existing
                .get(
                    &PageIdentity::new(workspace.template(), PageType::Concept, &concept.name)?
                        .key(),
                )
                .map(|page| page.path.clone())
                .unwrap_or_else(|| unique_path(workspace, PageType::Concept, &concept.name));
            concept_pages.push((path, concept.name.clone()));
        }
        let mut related_pages = RelatedPages::new(workspace.template());
        for page in existing.values() {
            related_pages.insert(
                page.document.frontmatter().page_type(),
                page.document.frontmatter().title(),
                &page.path,
            );
        }
        for (path, title) in &entity_pages {
            related_pages.insert(PageType::Entity, title, path);
        }
        for (path, title) in &concept_pages {
            related_pages.insert(PageType::Concept, title, path);
        }
        let source_body = render_source_body(
            &analysis.title,
            &analysis.summary,
            &entity_pages,
            &concept_pages,
            &raw_path,
            original_name,
        )?;
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
            Some(raw_reference),
            &source_body,
        )?;

        let mut files = BTreeMap::new();
        let mut pages = Vec::new();
        let mut touched = vec![ResourceRevision::absent(ResourceId::new(&raw_path)?)];
        let mut created = vec![source_page.clone()];
        let mut updated = Vec::new();
        files.insert(raw_path.clone(), markdown.to_owned().into_bytes());
        created.push(raw_path.clone());
        files.insert(source_page.clone(), source_markdown.clone().into_bytes());
        touched.push(ResourceRevision::absent(ResourceId::page(
            corpusbot_core::WikiPath::parse(&source_page)?,
        )));
        pages.push(PageFile {
            path: source_page.clone(),
            markdown: source_markdown,
            title: source_title.clone(),
            page_type: PageType::Source.as_str().to_owned(),
            updated_at: iso_date(now),
        });

        for entity in &entities {
            let (path, markdown, is_new) = entity_page(
                workspace,
                &existing,
                entity,
                &source_ref,
                &source_link,
                &related_pages,
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

        for concept in &concepts {
            let (path, markdown, is_new) = concept_page(
                workspace,
                &existing,
                concept,
                &source_ref,
                &source_link,
                &related_pages,
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

        let mut index_pages = existing
            .values()
            .map(|page| {
                (
                    page.path.clone(),
                    page.document.frontmatter().title().to_owned(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for (path, title) in entity_pages.iter().chain(&concept_pages) {
            index_pages.insert(path.clone(), title.clone());
        }
        index_pages.insert(source_page.clone(), source_title.clone());
        let index = render_index(&index_pages);
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
            run_id,
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
}

fn validated_source_name(value: &str) -> Result<&str> {
    let valid =
        !value.is_empty() && !matches!(value, "." | "..") && !value.contains(['/', '\\', '\0']);
    if !valid {
        return Err(IngestError::InvalidSourceName);
    }
    Ok(value)
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

fn unique_entities(
    template: corpusbot_core::Template,
    plan: &DraftPlan,
) -> Result<Vec<EntityDraft>> {
    let mut selected = Vec::new();
    let mut identities = BTreeMap::new();
    for entity in &plan.entities {
        let identity = PageIdentity::new(template, PageType::Entity, &entity.name)?;
        if identities.insert(identity.key(), ()).is_none() {
            selected.push(entity.clone());
        }
    }
    Ok(selected)
}

fn unique_concepts(
    template: corpusbot_core::Template,
    plan: &DraftPlan,
) -> Result<Vec<ConceptDraft>> {
    let mut selected = Vec::new();
    let mut identities = BTreeMap::new();
    for concept in &plan.concepts {
        let identity = PageIdentity::new(template, PageType::Concept, &concept.name)?;
        if identities.insert(identity.key(), ()).is_none() {
            selected.push(concept.clone());
        }
    }
    Ok(selected)
}

fn validate_plan(plan: &DraftPlan, template: corpusbot_core::Template, attempt: u32) -> Result<()> {
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
    if let Some(reason) = validate_draft_plan(plan, template) {
        return Err(IngestError::SelfAudit(format!(
            "attempt {attempt}: {reason}"
        )));
    }
    Ok(())
}

struct ExistingWikiPage {
    path: String,
    document: WikiDoc,
}

struct RelatedPages {
    template: corpusbot_core::Template,
    paths: BTreeMap<String, Vec<(PageType, String)>>,
}

impl RelatedPages {
    fn new(template: corpusbot_core::Template) -> Self {
        Self {
            template,
            paths: BTreeMap::new(),
        }
    }

    fn insert(&mut self, page_type: PageType, title: &str, path: &str) {
        let Ok(identity) = PageIdentity::new(self.template, page_type, title) else {
            return;
        };
        let candidates = self
            .paths
            .entry(identity.canonical_name().to_owned())
            .or_default();
        candidates.retain(|(candidate, _)| *candidate != page_type);
        candidates.push((page_type, path.to_owned()));
    }

    fn resolve(&self, name: &str) -> Option<Wikilink> {
        let normalized = PageIdentity::normalize(name).ok()?;
        let candidates = self.paths.get(&normalized)?;
        if candidates.len() != 1 {
            return None;
        }
        let path = &candidates[0].1;
        Wikilink::with_alias(path, name.trim()).ok()
    }

    fn resolve_all(&self, names: &[String]) -> Vec<Wikilink> {
        let mut links = Vec::new();
        for name in names {
            if let Some(link) = self.resolve(name) {
                push_wikilink(&mut links, &link);
            }
        }
        links
    }
}

fn render_sections(sections: &[DraftSection], heading_level: usize) -> String {
    let prefix = "#".repeat(heading_level);
    sections
        .iter()
        .map(|section| {
            let mut output = format!("\n{prefix} {}\n\n", section.heading.trim());
            for paragraph in &section.paragraphs {
                let paragraph = paragraph.trim();
                if !paragraph.is_empty() {
                    output.push_str(paragraph);
                    output.push_str("\n\n");
                }
            }
            if !section.bullets.is_empty() {
                for bullet in &section.bullets {
                    let bullet = bullet.trim();
                    if !bullet.is_empty() {
                        output.push_str(&format!("- {bullet}\n"));
                    }
                }
                output.push('\n');
            }
            output
        })
        .collect()
}

fn render_related_pages(links: &[Wikilink]) -> String {
    if links.is_empty() {
        return String::new();
    }
    let items = links
        .iter()
        .map(|link| format!("- {}\n", link.render()))
        .collect::<String>();
    format!("\n## Related pages\n\n{items}")
}

fn render_source_body(
    title: &str,
    summary: &str,
    entities: &[(String, String)],
    concepts: &[(String, String)],
    raw_path: &str,
    original_name: &str,
) -> Result<String> {
    let mut output = format!("# {title}\n\n{summary}\n");
    if !entities.is_empty() || !concepts.is_empty() {
        output.push_str("\n## Extracted pages\n");
        if !entities.is_empty() {
            output.push_str("\n### Entities\n");
            for (path, name) in entities {
                output.push_str(&format!("- [[{path}|{name}]]\n"));
            }
        }
        if !concepts.is_empty() {
            output.push_str("\n### Concepts\n");
            for (path, name) in concepts {
                output.push_str(&format!("- [[{path}|{name}]]\n"));
            }
        }
    }
    output.push_str("\n\n## Captured source\n\n");
    output.push_str(&format!(
        "Original file: {}\n",
        Wikilink::with_alias(raw_path, original_name)?.render()
    ));
    Ok(output)
}

fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut clean = Vec::new();
    for tag in tags {
        let normalized = tag.trim().to_lowercase();
        let tag = slugify(&normalized);
        if !tag.is_empty() {
            push_unique(&mut clean, tag);
        }
    }
    clean
}

fn clean_aliases(aliases: &[String]) -> Result<Vec<String>> {
    let mut clean = Vec::new();
    for alias in aliases {
        if alias.trim().is_empty() {
            continue;
        }
        push_unique(&mut clean, PageIdentity::normalize(alias)?);
    }
    Ok(clean)
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
    raw: Option<RawReference>,
    body: &str,
) -> Result<String> {
    let date = IsoDate::parse(iso_date(now))?;
    let frontmatter = Frontmatter::new(
        page_type, title, date, date, tags, aliases, related, sources,
    )?;
    let frontmatter = frontmatter.with_raw(raw)?;
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
    entity: &EntityDraft,
    source_ref: &SourceRef,
    source_link: &Wikilink,
    related_pages: &RelatedPages,
    now: OffsetDateTime,
) -> Result<(String, String, bool)> {
    let name = entity.name.as_str();
    let related_links = related_pages.resolve_all(&entity.related);
    let tags = clean_tags(&entity.tags);
    let aliases = clean_aliases(&entity.aliases)?;
    let Some(page) = find_existing(workspace, existing, PageType::Entity, name) else {
        let path = unique_path(workspace, PageType::Entity, name);
        let body = format!(
            "# {name}\n\n{}\n{}{}\n\n## From {}\n\nSource: {}\n",
            entity.summary,
            render_sections(&entity.sections, 2),
            render_related_pages(&related_links),
            source_ref.title(),
            source_link.render()
        );
        let markdown = render_page(
            workspace,
            &path,
            PageType::Entity,
            name,
            now,
            tags,
            aliases,
            {
                let mut related = vec![source_link.clone()];
                for link in related_links {
                    push_wikilink(&mut related, &link);
                }
                related
            },
            vec![source_ref.clone()],
            None,
            &body,
        )?;
        return Ok((path, markdown, true));
    };

    let old = page.document.clone();
    let old_frontmatter = old.frontmatter();
    let mut tags = old_frontmatter.tags().to_vec();
    for tag in clean_tags(&entity.tags) {
        push_unique(&mut tags, tag);
    }
    let mut normalized_aliases = old_frontmatter.aliases().to_vec();
    for alias in clean_aliases(&entity.aliases)? {
        push_unique(&mut normalized_aliases, alias);
    }
    let mut related = old_frontmatter.related().to_vec();
    let mut sources = old_frontmatter.sources().to_vec();
    push_wikilink(&mut related, source_link);
    for link in related_links {
        push_wikilink(&mut related, &link);
    }
    push_source(&mut sources, source_ref.clone())?;
    let source_update = format!(
        "{}\n{}",
        entity.summary,
        render_sections(&entity.sections, 3)
    );
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
        source_update,
        source_link.render()
    );
    Ok((page.path.clone(), markdown, false))
}

#[allow(clippy::too_many_arguments)]
fn concept_page(
    workspace: &Workspace,
    existing: &BTreeMap<String, ExistingWikiPage>,
    concept: &ConceptDraft,
    source_ref: &SourceRef,
    source_link: &Wikilink,
    related_pages: &RelatedPages,
    now: OffsetDateTime,
) -> Result<(String, String, bool)> {
    let name = concept.name.as_str();
    let related_links = related_pages.resolve_all(&concept.related);
    let tags = clean_tags(&concept.tags);
    let aliases = clean_aliases(&concept.aliases)?;
    let Some(page) = find_existing(workspace, existing, PageType::Concept, name) else {
        let path = unique_path(workspace, PageType::Concept, name);
        let body = format!(
            "# {name}\n\n{}\n{}{}\n\n## From {}\n\nSource: {}\n",
            concept.definition,
            render_sections(&concept.sections, 2),
            render_related_pages(&related_links),
            source_ref.title(),
            source_link.render()
        );
        let markdown = render_page(
            workspace,
            &path,
            PageType::Concept,
            name,
            now,
            tags,
            aliases,
            {
                let mut related = vec![source_link.clone()];
                for link in related_links {
                    push_wikilink(&mut related, &link);
                }
                related
            },
            vec![source_ref.clone()],
            None,
            &body,
        )?;
        return Ok((path, markdown, true));
    };

    let old = page.document.clone();
    let old_frontmatter = old.frontmatter();
    let mut tags = old_frontmatter.tags().to_vec();
    for tag in clean_tags(&concept.tags) {
        push_unique(&mut tags, tag);
    }
    let mut aliases = old_frontmatter.aliases().to_vec();
    for alias in clean_aliases(&concept.aliases)? {
        push_unique(&mut aliases, alias);
    }
    let mut related = old_frontmatter.related().to_vec();
    let mut sources = old_frontmatter.sources().to_vec();
    push_wikilink(&mut related, source_link);
    for link in related_links {
        push_wikilink(&mut related, &link);
    }
    push_source(&mut sources, source_ref.clone())?;
    let source_update = format!(
        "{}\n{}",
        concept.definition,
        render_sections(&concept.sections, 3)
    );
    let date = IsoDate::parse(iso_date(now))?;
    let frontmatter = Frontmatter::new(
        PageType::Concept,
        old_frontmatter.title(),
        old_frontmatter.created(),
        date,
        tags,
        aliases,
        related,
        sources,
    )?;
    let markdown = format!(
        "---\n{}---\n\n{}\n\n## From {}\n\n{}\n\nSource: {}\n",
        serde_yaml::to_string(&frontmatter)?,
        old.body(),
        source_ref.title(),
        source_update,
        source_link.render()
    );
    Ok((page.path.clone(), markdown, false))
}

fn render_index(pages: &BTreeMap<String, String>) -> String {
    let mut groups = BTreeMap::<String, Vec<(String, String)>>::new();
    for (path, title) in pages {
        let directory = path.split('/').take(2).collect::<Vec<_>>().join("/");
        groups
            .entry(directory)
            .or_default()
            .push((path.clone(), title.clone()));
    }

    let mut lines = vec!["# Index".to_owned()];
    for (directory, mut group) in groups {
        group.sort_by(|left, right| {
            left.1
                .to_lowercase()
                .cmp(&right.1.to_lowercase())
                .then(left.0.cmp(&right.0))
        });
        lines.push(String::new());
        lines.push(format!(
            "## {}",
            directory.strip_prefix("wiki/").unwrap_or(&directory)
        ));
        lines.push(String::new());
        for (path, title) in group {
            lines.push(format!("- [[{path}|{title}]]"));
        }
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
        {
          "name": "Raft",
          "aliases": ["Raft consensus"],
          "summary": "A leader-based consensus algorithm used to coordinate replicated state.",
          "tags": ["consensus"],
          "related": ["Leader Election"],
          "sections": [
            {
              "heading": "Role",
              "paragraphs": ["Raft coordinates a replicated state machine through an elected leader."]
            },
            {
              "heading": "Evidence",
              "bullets": ["A candidate needs a majority of votes."]
            }
          ]
        }
      ],
      "concepts": [
        {
          "name": "Leader Election",
          "definition": "The process of selecting a coordinator for replicated log entries.",
          "aliases": ["leader selection"],
          "tags": ["consensus", "election"],
          "related": ["Raft"],
          "sections": [
            {
              "heading": "Mechanism",
              "paragraphs": ["Candidates request votes and become leader after winning a majority."]
            },
            {
              "heading": "Failure behavior",
              "bullets": ["A candidate without a majority cannot become leader."]
            }
          ]
        }
      ]
    }"#;

    #[tokio::test]
    async fn reports_real_workflow_stages() -> Result<()> {
        let root = tempfile::tempdir()?;
        corpusbot_store::Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let source = root.path().join("raft.md");
        std::fs::write(&source, SOURCE)?;

        let stages = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let callback_stages = std::sync::Arc::clone(&stages);
        let ingestor = Ingestor::new(FakeLlmClient::new([ANALYSIS, DRAFTS])).with_progress(
            std::sync::Arc::new(move |update: IngestProgressUpdate| {
                callback_stages
                    .lock()
                    .expect("progress mutex")
                    .push((update.stage, update.run_id));
            }),
        );

        let result = ingestor.ingest_file(&workspace, &source).await?;
        assert!(matches!(result, IngestResult::Committed { .. }));
        let stages = stages.lock().expect("progress mutex").clone();
        assert_eq!(
            stages,
            vec![
                (IngestStage::Analyze, stages[0].1.clone()),
                (IngestStage::Draft, stages[0].1.clone()),
                (IngestStage::Commit, stages[0].1.clone()),
            ]
        );
        assert!(
            stages
                .iter()
                .all(|(_, run_id)| run_id.starts_with("ingest_"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn strips_frontmatter_from_llm_prompts() -> Result<()> {
        let root = tempfile::tempdir()?;
        corpusbot_store::Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let source = root.path().join("raft.md");
        std::fs::write(
            &source,
            "---\ninternal_note: keep out of prompts\n---\n\n# Raft\n\nRaft elects a leader.",
        )?;

        let client = FakeLlmClient::new([ANALYSIS, DRAFTS]);
        let calls_client = client.clone();
        let result = Ingestor::new(client)
            .ingest_file(&workspace, &source)
            .await?;
        assert!(matches!(result, IngestResult::Committed { .. }));

        let prompts = calls_client
            .calls()
            .into_iter()
            .filter(|request| request.operation != "connection-test")
            .map(|request| request.prompt)
            .collect::<Vec<_>>();
        assert_eq!(prompts.len(), 2);
        assert!(
            prompts
                .iter()
                .all(|prompt| { !prompt.contains("internal_note") && prompt.contains("# Raft") })
        );
        Ok(())
    }

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
        assert_eq!(created_paths.len(), 4);
        assert!(root.path().join(&source_page).exists());
        let raw_path = format!("raw/{}/raft.md", hex(SOURCE.as_bytes()));
        assert!(root.path().join(&raw_path).exists());
        assert_eq!(
            std::fs::read(root.path().join(&raw_path))?,
            SOURCE.as_bytes()
        );
        let source_markdown = std::fs::read_to_string(root.path().join(&source_page))?;
        assert!(source_markdown.contains(&format!("path: {raw_path}")));
        assert!(source_markdown.contains(&hex(SOURCE.as_bytes())));
        assert!(source_markdown.contains(&format!("Original file: [[{raw_path}|raft.md]]")));
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
    async fn changed_source_creates_a_new_source_version() -> Result<()> {
        let root = tempfile::tempdir()?;
        corpusbot_store::Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let source = root.path().join("raft.md");
        std::fs::write(&source, SOURCE)?;

        let first = Ingestor::new(FakeLlmClient::new([ANALYSIS, DRAFTS]))
            .ingest_file(&workspace, &source)
            .await?;
        let IngestResult::Committed {
            source_page: first_source_page,
            source_version_id: first_version_id,
            ..
        } = first
        else {
            panic!("first ingest should commit");
        };
        let original_source_page = workspace.read_page(&first_source_page)?;

        std::fs::write(
            &source,
            format!("{SOURCE}\n\nA second revision adds commit semantics."),
        )?;
        let second = Ingestor::new(FakeLlmClient::new([ANALYSIS, DRAFTS]))
            .ingest_file(&workspace, &source)
            .await?;
        let IngestResult::Committed {
            source_page: second_source_page,
            source_version_id: second_version_id,
            ..
        } = second
        else {
            panic!("changed source should create a new version");
        };

        assert_ne!(first_version_id, second_version_id);
        assert_ne!(first_source_page, second_source_page);
        assert_eq!(
            workspace.read_page(&first_source_page)?,
            original_source_page
        );
        assert!(root.path().join(&second_source_page).exists());
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
    async fn content_import_rejects_path_like_names() -> Result<()> {
        let root = tempfile::tempdir()?;
        corpusbot_store::Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let ingestor = Ingestor::new(FakeLlmClient::new(Vec::<String>::new()));

        for name in ["../escape.md", "/etc/passwd", "nested/source.md", "..", ""] {
            assert!(matches!(
                ingestor.ingest_content(&workspace, name, SOURCE).await,
                Err(IngestError::InvalidSourceName)
            ));
        }
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
