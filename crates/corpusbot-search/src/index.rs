use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{DateOptions, INDEXED, IndexRecordOption, Schema, TextFieldIndexing, Value};
use tantivy::tokenizer::{NgramTokenizer, TextAnalyzer};
use tantivy::{Index, TantivyDocument};
use time::OffsetDateTime;

use crate::error::{Result, SearchError};

pub const CJK_TOKENIZER: &str = "cjk_ngram";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchDocument {
    pub path: String,
    pub title: String,
    pub page_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub body: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub page_type: String,
    pub tags: Vec<String>,
    pub score: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchGeneration {
    pub generation_id: String,
    pub document_count: usize,
}

#[derive(Clone)]
pub struct SearchIndex {
    root: PathBuf,
}

pub struct SearchSchema {
    pub schema: Schema,
    pub path: tantivy::schema::Field,
    pub title: tantivy::schema::Field,
    pub page_type: tantivy::schema::Field,
    pub tags: tantivy::schema::Field,
    pub body: tantivy::schema::Field,
    pub updated_at: tantivy::schema::Field,
}

impl SearchIndex {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn schema() -> SearchSchema {
        let mut builder = Schema::builder();
        let path = builder.add_text_field("path", string_stored());
        let title = builder.add_text_field("title", cjk_text().set_stored());
        let page_type = builder.add_text_field("page_type", string_stored());
        let tags = builder.add_text_field("tags", cjk_text());
        let body = builder.add_text_field("body", cjk_text());
        let updated_at = builder.add_date_field("updated_at", date_stored());
        SearchSchema {
            schema: builder.build(),
            path,
            title,
            page_type,
            tags,
            body,
            updated_at,
        }
    }

    pub fn rebuild(&self, documents: &[SearchDocument]) -> Result<SearchGeneration> {
        let generation_id = format!(
            "{:x}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| SearchError::InvalidGeneration(error.to_string()))?
                .as_nanos(),
            documents.len()
        );
        let generations = self.root.join("generations");
        std::fs::create_dir_all(&generations)?;
        let generation_path = generations.join(&generation_id);
        std::fs::create_dir_all(&generation_path)?;
        let schema = Self::schema();
        let index = Index::create_in_dir(&generation_path, schema.schema.clone())?;
        assert!(
            generation_path.exists(),
            "generation path must exist immediately after index creation"
        );
        index.tokenizers().register(CJK_TOKENIZER, cjk_tokenizer());
        let mut writer = index.writer(50_000_000)?;
        for document in documents {
            writer.add_document(self.tantivy_document(&schema, document))?;
        }
        writer.commit()?;
        std::fs::write(
            generation_path.join("generation.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "generation_id": generation_id,
                "document_count": documents.len(),
            }))?,
        )?;
        assert!(
            generation_path.exists(),
            "generation path disappeared before pointer switch"
        );
        atomic_write(&self.root.join("current"), generation_id.as_bytes())?;
        Ok(SearchGeneration {
            generation_id,
            document_count: documents.len(),
        })
    }

    pub fn current_generation(&self) -> Result<SearchGeneration> {
        let id = std::fs::read_to_string(self.root.join("current"))?
            .trim()
            .to_owned();
        if id.is_empty() || id.contains(['/', '\\', '\0']) {
            return Err(SearchError::InvalidGeneration(id));
        }
        let manifest = self
            .root
            .join("generations")
            .join(&id)
            .join("generation.json");
        let raw = std::fs::read_to_string(manifest)?;
        serde_json::from_str(&raw).map_err(Into::into)
    }

    pub fn open_current(&self) -> Result<Index> {
        let generation = self.current_generation()?;
        let index =
            Index::open_in_dir(self.root.join("generations").join(generation.generation_id))?;
        index.tokenizers().register(CJK_TOKENIZER, cjk_tokenizer());
        Ok(index)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let Ok(_generation) = self.current_generation() else {
            return Ok(Vec::new());
        };
        let index = self.open_current()?;
        let schema = Self::schema();
        let parser = QueryParser::for_index(
            &index,
            vec![schema.title, schema.page_type, schema.tags, schema.body],
        );
        let query = parser.parse_query_lenient(query).0;
        let reader = index.reader()?;
        let searcher = reader.searcher();
        let hits = searcher.search(&*query, &TopDocs::with_limit(limit).order_by_score())?;

        Ok(hits
            .into_iter()
            .filter_map(|(score, address)| {
                let document = searcher.doc::<TantivyDocument>(address).ok()?;
                Some(SearchHit {
                    path: stored(&document, schema.path)?,
                    title: stored(&document, schema.title)?,
                    page_type: stored(&document, schema.page_type)?,
                    tags: multi(&document, schema.tags),
                    score,
                })
            })
            .collect())
    }

    fn tantivy_document(
        &self,
        schema: &SearchSchema,
        document: &SearchDocument,
    ) -> TantivyDocument {
        let timestamp = OffsetDateTime::parse(
            &document.updated_at,
            &time::format_description::well_known::Rfc3339,
        )
        .map(|value| value.unix_timestamp_nanos() as i64)
        .unwrap_or_default();

        let mut result = TantivyDocument::default();
        result.add_text(schema.path, &document.path);
        result.add_text(schema.title, &document.title);
        result.add_text(schema.page_type, &document.page_type);
        for tag in &document.tags {
            result.add_text(schema.tags, tag);
        }
        result.add_text(schema.body, &document.body);
        result.add_date(
            schema.updated_at,
            tantivy::DateTime::from_timestamp_nanos(timestamp),
        );
        result
    }
}

fn cjk_tokenizer() -> TextAnalyzer {
    TextAnalyzer::from(NgramTokenizer::new(1, 2, false).expect("valid tokenizer range"))
}

fn cjk_text() -> tantivy::schema::TextOptions {
    tantivy::schema::TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(CJK_TOKENIZER)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored()
}

fn string_stored() -> tantivy::schema::TextOptions {
    tantivy::schema::TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("raw")
                .set_index_option(IndexRecordOption::Basic),
        )
        .set_stored()
}

fn date_stored() -> DateOptions {
    DateOptions::from(INDEXED)
        .set_stored()
        .set_precision(tantivy::schema::DateTimePrecision::Seconds)
}

fn stored(document: &TantivyDocument, field: tantivy::schema::Field) -> Option<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn multi(document: &TantivyDocument, field: tantivy::schema::Field) -> Vec<String> {
    document
        .get_all(field)
        .filter_map(|value| value.as_str())
        .map(ToOwned::to_owned)
        .collect()
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = tempfile::NamedTempFile::new_in(path.parent().expect("pointer parent"))?;
    std::io::Write::write_all(&mut file, content)?;
    file.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuilds_searches_and_returns_hits() -> Result<()> {
        let root = tempfile::tempdir()?;
        let index = SearchIndex::new(root.path().join("tantivy"));
        let empty = match index.search("raft", 8) {
            Ok(value) => value,
            Err(error) => panic!("initial search failed: {error:?}"),
        };
        assert!(empty.is_empty());

        let documents = vec![
            SearchDocument {
                path: "wiki/entities/Raft.md".to_owned(),
                title: "Raft".to_owned(),
                page_type: "entity".to_owned(),
                tags: vec!["distributed-systems".to_owned()],
                body: "Raft elects a leader before replicating log entries.".to_owned(),
                updated_at: "2026-09-08T00:00:00Z".to_owned(),
            },
            SearchDocument {
                path: "wiki/concepts/LeaderElection.md".to_owned(),
                title: "Leader Election".to_owned(),
                page_type: "concept".to_owned(),
                tags: vec!["consensus".to_owned()],
                body: "选举在候选人获得多数投票后完成。".to_owned(),
                updated_at: "2026-09-08T00:00:00Z".to_owned(),
            },
        ];
        let generation = match index.rebuild(&documents) {
            Ok(value) => value,
            Err(error) => panic!("rebuild failed: {error:?}"),
        };
        assert_eq!(generation.document_count, 2);
        assert_eq!(
            index.current_generation()?.generation_id,
            generation.generation_id
        );

        let hits = match index.search("Raft leader", 8) {
            Ok(value) => value,
            Err(error) => panic!("search failed: {error:?}"),
        };
        assert!(hits[0].path == "wiki/entities/Raft.md");
        assert!(
            index
                .search("选举", 8)
                .unwrap_or_else(|error| panic!("chinese search failed: {error:?}"))
                .iter()
                .any(|hit| hit.path == "wiki/concepts/LeaderElection.md")
        );
        assert!(index.search("does-not-exist", 8)?.is_empty());
        Ok(())
    }
}
