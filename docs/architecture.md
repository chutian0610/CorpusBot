# Architecture Map

CorpusBot is a local-first compiler for research notes. A source enters as an
immutable Markdown Source Version, is analyzed into a validated Draft, and only
then is committed with the Wiki pages, index, log, source metadata, and search
state. Disk content under `wiki/` and `raw/` remains authoritative.

## Runtime Surfaces

| Surface           | Role                                                                                                                               |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Desktop shell     | Tauri commands provide workspace actions and background ingest. React is a desktop-height application shell, not a scrolling page. |
| Local HTTP server | The `corpusbot-server` binary exposes the same command layer to the browser UI for local development and Playwright.               |
| CLI               | A thin automation and development entry point over the same engine crates.                                                         |

The desktop and local-server paths deliberately share command semantics so a
workspace has one transaction, audit, and recovery model.

## Crate Boundaries

| Crate              | Owns                                                                                                     |
| ------------------ | -------------------------------------------------------------------------------------------------------- |
| `corpusbot-core`   | Domain types, Markdown/frontmatter parsing, page identity, path and citation invariants.                 |
| `corpusbot-store`  | SQLite metadata, workspace state, revision manifests, CAS commits, recovery, and index refresh.          |
| `corpusbot-vcs`    | Local Git snapshots, history, restore, and transaction recovery primitives.                              |
| `corpusbot-search` | The rebuildable Tantivy/BM25 index.                                                                      |
| `corpusbot-ingest` | Ingest orchestration, page rendering/merging, final draft validation, and atomic commit preparation.     |
| `corpusbot-agent`  | Provider configuration, LLM adapters, audit artifacts, typed workflow kernel, prompts, and JSON parsing. |
| `corpusbot-lint`   | Deterministic health checks. Reports issues; it does not repair or authorize writes.                     |
| `corpusbot-cli`    | Command-line argument parsing and calls into the engine crates.                                          |
| `src-tauri`        | Desktop/local command adapters, job state for the running process, logging, and server bootstrap.        |

Frontend state lives in `src/store/useWorkspaceStore.ts`; backend-specific
transport details are isolated in `src/lib/api.ts`.

## Ingest Flow

1. The ingest guard verifies recovery completion and a clean tracked workspace
   before reading or analyzing source content.
2. Markdown bytes become an immutable Source Version keyed by SHA-256.
3. `corpusbot-agent` analyzes the source, normalizes the candidate inventory,
   generates bounded page batches, and persists every attempt's request,
   available provider response, telemetry, and decision under
   `.wiki-db/audit/`.
4. `corpusbot-ingest` performs a final structural draft check, renders new or
   source-attributed merged pages, and records expected resource revisions.
5. `corpusbot-store` validates CAS revisions, applies files, updates derived
   metadata, commits a snapshot, reconciles the run, and rebuilds search.

LLM output never selects an unsafe transition or writes directly to `wiki/`.
Provider text is typed, repaired only at deterministic parse boundaries, and
validated before it becomes part of a Draft.

## Source Of Truth

Use these documents for detailed decisions rather than duplicating them here:

- Product and acceptance scope: [MVP_PLAN.md](MVP_PLAN.md),
  [MVP_ACCEPTANCE.md](MVP_ACCEPTANCE.md), and [ALPHA_PLAN.md](ALPHA_PLAN.md).
- Extraction model: [ingest-extraction.md](ingest-extraction.md).
- Domain vocabulary: [../../CONTEXT.md](../../CONTEXT.md).
- Architecture decisions: [adr](adr).
- Operator commands and limitations: [../../README.md](../../README.md).
