use corpusbot_agent::{
    ANALYZE_PROMPT_ID, DRAFT_PROMPT_ID, FakeLlmClient, QUERY_PROMPT_ID, QueryContextPage,
    SourceAgent,
};
use corpusbot_core::Template;
use corpusbot_ingest::Ingestor;
use corpusbot_lint::engine::run_lint;
use corpusbot_search::SearchIndex;
use corpusbot_store::Workspace;

const SOURCE_ONE: &str = r#"# Raft

Raft elects a leader before replicating log entries. A candidate needs a majority."#;

const SOURCE_TWO: &str = r#"# Raft evidence

Additional evidence shows that a candidate becomes leader after receiving majority votes."#;

const ANALYSIS_ONE: &str = r#"{
  "title": "Raft",
  "summary": "Raft is a leader-based consensus algorithm.",
  "entities": [
    {
      "name": "Raft",
      "aliases": ["Raft consensus"],
      "summary": "A leader-based consensus algorithm."
    }
  ],
  "concepts": [
    {
      "name": "Leader Election",
      "definition": "Selecting a coordinator before replication."
    }
  ]
}"#;

const DRAFTS_ONE: &str = r#"{
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
      "definition": "Selecting a coordinator before replicated log entries are applied.",
      "aliases": ["leader selection"],
      "tags": ["consensus", "election"],
      "related": ["Raft"],
      "sections": [
        {
          "heading": "Mechanism",
          "paragraphs": ["Candidates request votes before they may coordinate replication."]
        },
        {
          "heading": "Failure behavior",
          "bullets": ["A candidate without a majority cannot become leader."]
        }
      ]
    }
  ]
}"#;

const ANALYSIS_TWO: &str = r#"{
  "title": "Raft Evidence",
  "summary": "Additional evidence confirms majority election.",
  "entities": [
    {
      "name": "Raft",
      "aliases": [],
      "summary": "Majority votes elect a candidate."
    }
  ],
  "concepts": [
    {
      "name": "Leader Election",
      "definition": "A majority vote makes a candidate leader."
    }
  ]
}"#;

const DRAFTS_TWO: &str = r#"{
  "source_summary": "Additional evidence confirms majority election.",
  "entities": [
    {
      "name": "Raft",
      "aliases": [],
      "summary": "Majority votes elect a candidate in the Raft consensus protocol.",
      "tags": ["consensus", "election"],
      "related": ["Leader Election"],
      "sections": [
        {
          "heading": "Evidence",
          "paragraphs": ["Additional evidence shows a candidate becomes leader after majority votes."]
        },
        {
          "heading": "Operational meaning",
          "bullets": ["Majority availability is required for election."]
        }
      ]
    }
  ],
  "concepts": [
    {
      "name": "Leader Election",
      "definition": "A majority vote makes a candidate leader and authorizes replication.",
      "aliases": [],
      "tags": ["election"],
      "related": ["Raft"],
      "sections": [
        {
          "heading": "Evidence",
          "paragraphs": ["The source states that majority votes make a candidate leader."]
        },
        {
          "heading": "Invariant",
          "bullets": ["Only a candidate with majority support becomes leader."]
        }
      ]
    }
  ]
}"#;

const ANSWER_TEMPLATE: &str = r#"{
  "answer": "Raft elects a leader through majority voting [1].",
  "citations": [
    {
      "path": ENTITY_PATH_JSON,
      "quote": "leader",
      "revision": REVISION_JSON
    }
  ]
}"#;

#[tokio::test]
async fn fake_llm_end_to_end_compiles_searches_answers_lints_and_restores()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    Workspace::init(root.path(), Template::Research)?;
    let workspace = Workspace::open(root.path(), Template::Research)?;

    let source_one = root.path().join("source-one.md");
    std::fs::write(&source_one, SOURCE_ONE)?;
    let first_client = FakeLlmClient::new([ANALYSIS_ONE, DRAFTS_ONE]);
    let first = Ingestor::new(first_client)
        .ingest_file(&workspace, &source_one)
        .await?;
    let corpusbot_ingest::IngestResult::Committed {
        created_paths: first_created_paths,
        snapshot_id: first_snapshot_id,
        ..
    } = first
    else {
        panic!("first ingest should commit");
    };
    let entity_path = first_created_paths
        .iter()
        .find(|path| path.starts_with("wiki/entities/"))
        .expect("first ingest should create an entity")
        .clone();
    let first_source_path = first_created_paths
        .iter()
        .find(|path| path.starts_with("wiki/sources/"))
        .expect("first ingest should create a source page")
        .clone();
    let original_entity = workspace.read_page(&entity_path)?;
    assert!(original_entity.contains("tags:"));
    assert!(original_entity.contains("- consensus"));
    assert!(original_entity.contains("aliases:"));
    assert!(original_entity.contains("- raft consensus"));
    assert!(original_entity.contains("A leader-based consensus algorithm used"));
    let first_source = workspace.read_page(&first_source_path)?;
    assert!(first_source.contains(&format!("[[{entity_path}|Raft]]")));
    let first_index = workspace.read_page("wiki/index.md")?;
    assert!(first_index.contains(&entity_path));
    assert!(first_index.contains("wiki/concepts/leader-election.md"));

    let source_two = root.path().join("source-two.md");
    std::fs::write(&source_two, SOURCE_TWO)?;
    let second_client = FakeLlmClient::new([ANALYSIS_TWO, DRAFTS_TWO]);
    eprintln!(
        "before second: exists={} content={:?}",
        workspace.paths().root.join(&entity_path).exists(),
        workspace.read_page(&entity_path)
    );
    let second = Ingestor::new(second_client)
        .ingest_file(&workspace, &source_two)
        .await?;
    let corpusbot_ingest::IngestResult::Committed {
        run_id: ref second_run_id,
        ref updated_paths,
        ..
    } = second
    else {
        panic!("second ingest should commit");
    };
    assert_eq!(updated_paths.len(), 2);
    assert!(updated_paths.contains(&entity_path.clone()));
    assert!(
        updated_paths
            .iter()
            .any(|path| path.starts_with("wiki/concepts/"))
    );
    let ingest_ledger = std::fs::read_to_string(
        root.path()
            .join(".wiki-db/audit")
            .join(second_run_id)
            .join("events.jsonl"),
    )?;
    assert!(ingest_ledger.contains(ANALYZE_PROMPT_ID));
    assert!(ingest_ledger.contains(DRAFT_PROMPT_ID));
    assert!(ingest_ledger.contains(r#""prompt_hash""#));

    let merged_entity = workspace.read_page(&entity_path)?;
    assert!(merged_entity.contains("A leader-based consensus algorithm used"));
    assert!(
        merged_entity.contains("Majority votes elect a candidate in the Raft consensus protocol."),
        "merged entity:\n{merged_entity}"
    );

    let index = SearchIndex::new(workspace.paths().search_index.clone());
    let hits = index.search("Raft leader", 8)?;
    assert!(hits.iter().any(|hit| hit.path == entity_path));

    let manifest = workspace.revision_manifest()?;
    let resource = corpusbot_core::ResourceId::new(&entity_path)?;
    let revision_json = manifest.expected(&resource).key();
    let entity_path_json = serde_json::to_string(&entity_path)?;
    let answer_json = ANSWER_TEMPLATE
        .replace("ENTITY_PATH_JSON", &entity_path_json)
        .replace("REVISION_JSON", &revision_json);
    let context = [QueryContextPage {
        path: entity_path.to_owned(),
        title: "Raft".to_owned(),
        page_type: "entity".to_owned(),
        revision: manifest.expected(&resource),
        revision_manifest_id: manifest.manifest_id().to_owned(),
        markdown: merged_entity.clone(),
    }];
    let answer_client = FakeLlmClient::new([answer_json]);
    let answer = SourceAgent::new(answer_client)
        .answer_question_audited(
            root.path(),
            "query_e2e",
            manifest.manifest_id(),
            "How does Raft elect a leader?",
            &context,
        )
        .await?;
    assert!(!answer.insufficient_evidence);
    assert_eq!(answer.citations.len(), 1);
    assert_eq!(answer.citations[0].path, entity_path);
    let query_ledger =
        std::fs::read_to_string(root.path().join(".wiki-db/audit/query_e2e/events.jsonl"))?;
    assert!(query_ledger.contains(r#""node":"retrieve_context""#));
    assert!(!query_ledger.contains(ANALYZE_PROMPT_ID));
    assert!(query_ledger.contains(QUERY_PROMPT_ID));

    let lint = run_lint(root.path(), Template::Research, manifest.manifest_id())?;
    assert_eq!(lint.summary.errors, 0);
    assert_eq!(
        lint.summary.warnings, 0,
        "unexpected lint warnings: {:?}",
        lint.issues
    );

    workspace.restore(&first_snapshot_id)?;
    let restored = workspace.read_page(&entity_path)?;
    assert_eq!(restored, original_entity);
    Ok(())
}

#[tokio::test]
async fn touched_resource_conflict_rejects_commit_without_partial_write()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    Workspace::init(root.path(), Template::Research)?;
    let workspace = Workspace::open(root.path(), Template::Research)?;
    std::fs::create_dir_all(root.path().join("wiki/entities"))?;
    let path = root.path().join("wiki/entities/raft.md");
    std::fs::write(&path, "original")?;
    workspace.snapshot("baseline")?;

    let resource =
        corpusbot_core::ResourceId::page(corpusbot_core::WikiPath::parse("wiki/entities/raft.md")?);
    let baseline = workspace.revision_manifest()?;
    std::fs::write(&path, "externally changed")?;

    let request = corpusbot_store::IngestCommitRequest {
        run_id: "conflict-run".to_owned(),
        message: "conflicting ingest".to_owned(),
        source: None,
        files: vec![("wiki/entities/raft.md".to_owned(), b"draft".to_vec())],
        pages: Vec::new(),
        touched: vec![corpusbot_core::ResourceRevision::content(
            resource.clone(),
            b"original",
        )],
        baseline_manifest: baseline,
    };

    assert!(workspace.commit_ingest(&request).is_err());
    assert_eq!(std::fs::read_to_string(&path)?, "externally changed");
    let _ = resource;
    Ok(())
}

#[tokio::test]
async fn untouched_concurrent_edit_survives_cas_commit() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    Workspace::init(root.path(), Template::Research)?;
    let workspace = Workspace::open(root.path(), Template::Research)?;
    std::fs::create_dir_all(root.path().join("wiki/entities"))?;
    std::fs::create_dir_all(root.path().join("wiki/notes"))?;
    let touched = root.path().join("wiki/entities/raft.md");
    let untouched = root.path().join("wiki/notes/parallel.md");
    std::fs::write(&touched, "original")?;
    std::fs::write(&untouched, "original untouched")?;
    workspace.snapshot("baseline")?;

    let resource =
        corpusbot_core::ResourceId::page(corpusbot_core::WikiPath::parse("wiki/entities/raft.md")?);
    let baseline = workspace.revision_manifest()?;
    std::fs::write(&untouched, "externally edited untouched page")?;

    let request = corpusbot_store::IngestCommitRequest {
        run_id: "untouched-edit-run".to_owned(),
        message: "ingest with untouched concurrent edit".to_owned(),
        source: None,
        files: vec![("wiki/entities/raft.md".to_owned(), b"draft".to_vec())],
        pages: Vec::new(),
        touched: vec![corpusbot_core::ResourceRevision::content(
            resource,
            b"original",
        )],
        baseline_manifest: baseline,
    };

    workspace.commit_ingest(&request)?;
    assert_eq!(
        workspace.read_page("wiki/notes/parallel.md")?,
        "externally edited untouched page"
    );
    Ok(())
}
