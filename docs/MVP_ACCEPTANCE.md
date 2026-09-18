# MVP Acceptance

Status: **release candidate**. The scope follows
[MVP_PLAN.md](MVP_PLAN.md); this matrix records how each Definition of Done
item is verified.

## Verification Commands

| Check                          | Command                                        | Expected result                                                                                                                                                              |
| ------------------------------ | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Full engineering gate          | `make check`                                   | Formatting, Clippy, Rust and frontend tests, TypeScript check, frontend build, local HTTP E2E, and CLI acceptance all pass.                                                  |
| CLI acceptance                 | `scripts/mvp-acceptance.sh`                    | All eight CLI commands run against a deterministic OpenAI-compatible local provider; query returns a valid path/quote/revision citation; snapshot/history/restore reconcile. |
| Real-provider citation quality | `OPENAI_API_KEY=... scripts/llm-evaluation.sh` | At least 8 of 10 answers retain verifiable citations. The recorded MiniMax-M3 run passed 10/10 after reasoning-provider parsing was hardened.                                |

## Definition of Done

| MVP requirement                                            | Status | Evidence                                                                                                                                                                                                                                                                         |
| ---------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust full quality gate                                     | ✅     | `make check` runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.                                                                                                                                              |
| All eight CLI commands on a fixture workspace              | ✅     | `scripts/mvp-acceptance.sh` exercises `init`, `status`, `ingest`, `query`, `lint`, `snapshot`, `history`, and `restore`; `final status` verifies the restored workspace.                                                                                                         |
| Desktop create/read/import/query/Lint/History/Restore loop | ✅     | `e2e/desktop.spec.ts` drives the React application against the real local HTTP backend with Playwright; deterministic model output is supplied by an E2E-only fake provider. Tauri IPC itself remains covered by Rust command/engine tests rather than a WebKit browser harness. |
| 5-10 sample Wiki; 8/10 verifiable citations                | ✅     | `fixtures/mvp-eval` contains 8 sources and 10 questions. The runner validates manifest, path, revision, and normalized quote for every surviving citation.                                                                                                                       |
| Real LLM and fake LLM paths                                | ✅     | Fake paths are covered by `corpusbot-ingest` E2E and workflow tests. The real path is covered by `scripts/llm-evaluation.sh`; the latest recorded run passed 10/10.                                                                                                              |
| User edit is disk-authoritative and refreshes hashes       | ✅     | Store workspace tests create files outside an ingest commit and assert status/hash-backed page counts refresh.                                                                                                                                                                   |
| Dirty ingest guard                                         | ✅     | `dirty_workspace_blocks_before_llm_calls` proves the guard fails before a fake provider call and leaves no raw mutation.                                                                                                                                                         |
| Resource CAS rejects conflicts without partial write       | ✅     | `touched_resource_conflict_rejects_commit_without_partial_write`.                                                                                                                                                                                                                |
| Untouched concurrent edit survives CAS commit              | ✅     | `untouched_concurrent_edit_survives_cas_commit`.                                                                                                                                                                                                                                 |
| Revision Manifest read consistency                         | ✅     | Query/Lint capture a manifest before reads; query answers and lint reports carry its ID. Citation revision tests reject unknown or changed revisions.                                                                                                                            |
| Fresh dedicated repository policy                          | ✅     | `init_rejects_existing_repositories`, `init_rejects_nonempty_roots`, and VCS main-branch initialization.                                                                                                                                                                         |
| Clean snapshot is a no-op                                  | ✅     | `clean_snapshot_returns_head_without_a_new_commit`.                                                                                                                                                                                                                              |
| Workspace Lock serializes mutations                        | ✅     | `workspace_lock_blocks_concurrent_mutations`.                                                                                                                                                                                                                                    |
| Snapshot point-in-time consistency                         | ✅     | `dirty_snapshot_freezes_capture_and_reports_later_changes`; captured content remains unchanged and `workspace_changed_after_capture` is reported.                                                                                                                                |
| Restore conflict and journal recovery                      | ✅     | Store tests cover pre-switch conflict, prepared-phase crash, and post-Git-switch crash recovery.                                                                                                                                                                                 |
| Workflow audit, bounded repair, attempt exhaustion         | ✅     | Agent workflow tests persist request/response artifacts, schema rejection, provider failure, retry, and two-attempt exhaustion.                                                                                                                                                  |
| Similar page warning without semantic merge                | ✅     | `detects_similar_titles_as_possible_duplicate`; Page Identity normalization tests do not merge merely similar names.                                                                                                                                                             |
| Changed source creates a new Source Version                | ✅     | `changed_source_creates_a_new_source_version`; the prior Source Page remains intact.                                                                                                                                                                                             |
| Git failure/crash/commit reconciliation                    | ✅     | Store tests cover pre-Git recovery, commit-present reconciliation, restore prepared/switched journal states, and touched-path backup/rollback.                                                                                                                                   |
| Illegal LLM output cannot control workflow                 | ✅     | Typed schema validators reject malformed output; transitions remain deterministic and LLM output never chooses the next node.                                                                                                                                                    |
| Illegal path rejection                                     | ✅     | Core path/resource tests reject traversal, empty, absolute, and out-of-scope paths.                                                                                                                                                                                              |
| Duplicate source import is a no-op                         | ✅     | `ingests_a_file_once` verifies SHA-256 deduplication.                                                                                                                                                                                                                            |
| README, known limitations, Alpha scope                     | ✅     | README links the known limitations and [ALPHA_PLAN.md](ALPHA_PLAN.md); this matrix is the acceptance record.                                                                                                                                                                     |

## Recorded Real-Provider Run

- Provider: MiniMax-M3 through an OpenAI-compatible Chat Completions endpoint.
- Corpus: 8 Markdown sources.
- Questions: 10.
- Initial result: 9/10 citations survived validation.
- Root cause of failure: reasoning-style output preceded JSON and one response was
  truncated; a retry also used unescaped quotes inside a JSON string.
- Resolution: parser strips `<think>` before JSON extraction, the query prompt
  constrains output format and quote escaping, and the query completion budget was
  increased.
- Final recorded result: **10/10 (100%)**.

The real run used a user-provided API key and was not repeated in CI. CI uses the
deterministic local provider in `scripts/mvp_mock_provider.py`.
