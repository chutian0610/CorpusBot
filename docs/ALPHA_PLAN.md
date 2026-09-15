# Alpha Plan

MVP establishes one safe local loop: Markdown in, audited Wiki compilation,
versioned workspace state, cited answers, health checks, and recovery. Alpha
should broaden scale, provenance, review, and integration without weakening the
existing transactional boundaries.

## Guiding Rules

1. Keep disk Wiki content authoritative.
2. Keep every write behind drafts, Resource CAS, and short workspace locks.
3. Keep LLM transitions deterministic; richer graphs may add branches, but must
   not let model output select unsafe control flow.
4. Keep derived state rebuildable; Tantivy, graph indexes, and caches may be
   regenerated but never become the source of truth.
5. Add concurrent work only after a persistent queue has explicit ownership and
   recovery semantics.

## Alpha 1: Scale and Real-World Input

Goal: support research corpora larger than a single-file demo.

| Item                         | Why now                                                  | Approach                                                                                                                             |
| ---------------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| PDF, EPUB, and HTML import   | Research evidence is rarely Markdown-only.               | Extract text/layout metadata first, keep the immutable original in `raw/`, and treat extraction as a deterministic pre-LLM pipeline. |
| Persistent ingest queue      | Desktop restarts and long imports need durable progress. | Persist queue items and run state, retain serial workers initially, and reuse the existing ingest journal as the commit boundary.    |
| File watching                | Researchers edit source folders continuously.            | Debounce changes, hash before enqueue, and never import a dirty workspace automatically.                                             |
| Bounded parallel compilation | Serial ingest becomes slow on larger corpora.            | Parallelize extraction and LLM preparation first; keep touched-page commit serialization and CAS as the correctness boundary.        |
| Index maintenance            | Full rebuild costs grow with corpus size.                | Move to segment-level Tantivy updates with generation compaction and corruption fallback to rebuild.                                 |
| Release distribution         | MVP is source-run.                                       | Produce signed/notarized desktop packages where available, plus release CI artifacts and upgrade notes.                              |

Exit criteria: 100+ mixed-format sources can be imported after restarts without
partial Wiki state; search remains queryable after interrupted updates; desktop
release artifacts install and open a workspace.

## Alpha 2: Provenance, Review, and Contradiction

Goal: turn a compiled Wiki into a research review surface.

| Item                             | Why now                                                                    | Approach                                                                                                               |
| -------------------------------- | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Claim ledger                     | Citations prove where text came from, not which claim is stable.           | Represent claims as reviewed resources with source evidence, status, and conflicts.                                    |
| Source version relationships     | MVP treats changed content as a new version but does not model succession. | Add replaces/is-superseded-by links and review states without deleting old versions.                                   |
| Review panel and human decisions | Researchers need accept/reject/edit workflows.                             | Use Wiki pages as the record and persisted review state for workflow progress; all edits retain CAS.                   |
| Contradiction detection          | Multi-source corpora expose conflicting evidence.                          | Start with deterministic comparisons of structured claims and citations; LLM proposals remain drafts requiring review. |
| Graph retrieval                  | Related-note traversal becomes brittle at scale.                           | Build a graph index and evaluate Personalized PageRank over Wikilinks, source links, and review links.                 |
| Graph view                       | Researchers need neighborhood and cluster context.                         | Render a read-only graph first; editing remains in Markdown.                                                           |

Exit criteria: a reviewer can trace a claim from answer to source version, mark
contradictions, preserve decisions in Git history, and retrieve a defensible
neighborhood without losing citation validity.

## Alpha 3: Integration and Automation

Goal: make CorpusBot usable from other research tools.

| Item                   | Why now                                                   | Approach                                                                                                            |
| ---------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| MCP Server             | Other assistants need a controlled local interface.       | Expose read/query/lint/status and explicitly confirmed import/save operations only.                                 |
| Deep Research workflow | Long research needs staged retrieval and synthesis.       | Use the workflow kernel with persisted state, human checkpoints, and bounded graph expansion.                       |
| Save-to-Wiki           | External findings should enter the same provenance model. | Import as a Source Version and compile through the existing draft/CAS path.                                         |
| Remote backup or sync  | Local Git is durable but isolated.                        | Start with explicit backup push/pull to a user-configured remote; do not auto-rebase or silently resolve conflicts. |
| Provider switching     | Researchers use local and hosted models.                  | Allow provider profiles and per-operation model choice while retaining typed validation and audit.                  |

Exit criteria: an external assistant can query a local corpus without bypassing
citation validation, and a Deep Research run can pause, resume, audit, and save
only reviewed material.

## Deferred Beyond Alpha

- Automatic conflict resolution and Git branch merging.
- Multi-template migration as a general product feature.
- Louvain/community coloring beyond basic graph visualization.
- Autonomous unattended edits to committed Wiki prose.
- Cloud-first collaboration; Alpha remains local-first.

## Alpha Guardrails

- No feature may write directly from provider output to Wiki files.
- No background worker may mutate a dirty workspace.
- No graph or answer feature may omit revision-aware citations.
- No sync operation may execute `reset --hard`, force-push, discard user edits,
  or delete local snapshots.
