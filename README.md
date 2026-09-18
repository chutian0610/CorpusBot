# CorpusBot

CorpusBot is a local-first knowledge-base engine that compiles Markdown sources into a maintained Wiki and answers questions with verifiable citations.

The MVP implementation plan is in [docs/MVP_PLAN.md](docs/MVP_PLAN.md), acceptance evidence in
[docs/MVP_ACCEPTANCE.md](docs/MVP_ACCEPTANCE.md), and the next scope in
[docs/ALPHA_PLAN.md](docs/ALPHA_PLAN.md). Architecture decisions are in [docs/adr](docs/adr).

## Requirements

- Rust 1.98
- Node.js 22
- pnpm 10

## Local development

```bash
pnpm install
pnpm dev
```

The Vite dev server listens on <http://127.0.0.1:1420>. By itself it is the
frontend only; use one of the backend modes below.

### Desktop shell

Run the desktop shell with:

```bash
pnpm tauri dev
```

Or use the equivalent Make target:

```bash
make dev-tauri
```

### Browser UI with the real backend

Start the Rust HTTP backend and the Vite frontend together:

```bash
make dev-local
```

Then open:

```text
http://127.0.0.1:1420
```

You can also run them in separate terminals:

```bash
pnpm dev:server
pnpm dev:web:local
```

`pnpm dev:web:local` selects Vite mode `local-backend`. Alternatively, set
`VITE_CORPUSBOT_BACKEND=local` in `.env.local`.

The local backend listens only on `127.0.0.1:1421`; Vite proxies `/api` to it.
This mode uses the same workspace storage and commands as Tauri, but the browser
cannot open the macOS directory picker. Use the workspace-path dialog instead.
The old `?backend=browser` fixture has been removed.

## Desktop UI workflow

The desktop shell has five primary views:

- **Wiki**: browse generated pages in a list and read formatted metadata beside
  the page body.
- **Documents**: browse imported source pages in an editor-style folder tree and
  inspect the entity/concept pages extracted from each document.
- **Ingest**: import Markdown and inspect run history, affected resources, the
  original source, generated source pages, and workflow events.
- **Health**: run deterministic checks and report content-page/error/warning
  counts. Generated `wiki/index.md` and `wiki/log.md` are excluded from the
  content-page count.
- **History**: inspect snapshots and restore after confirmation.

The workspace launcher remembers recent paths locally. Deleting a launcher
entry removes only that history item, not the underlying workspace.

Ingest runs as a background job on the desktop and local-server modes. The job
status is held by the running process; restart recovery of queued UI jobs is not
implemented. Committed ingest runs remain durable in workspace storage.

## CLI workflow

Create a fresh research workspace:

```bash
cargo run -p corpusbot-cli -- init --root /path/to/research-wiki --template research
```

Import a Markdown source:

```bash
cargo run -p corpusbot-cli -- ingest --root /path/to/research-wiki --file ./paper.md
```

Then inspect health and ask a cited question:

```bash
cargo run -p corpusbot-cli -- lint --root /path/to/research-wiki --format table
cargo run -p corpusbot-cli -- query --root /path/to/research-wiki --question "What is Raft?"
```

Snapshots and restore:

```bash
cargo run -p corpusbot-cli -- snapshot --root /path/to/research-wiki --message "before edits"
cargo run -p corpusbot-cli -- history --root /path/to/research-wiki
cargo run -p corpusbot-cli -- restore --root /path/to/research-wiki --snapshot <snapshot-id> --yes
```

## LLM configuration

The desktop Settings view is the runtime source of truth. It writes an
OpenAI-compatible configuration to the app-level SQLite database at
`~/.corpusbot/daemon.db`. The API key is stored privately and is not displayed
again. Environment variables do not override saved app settings at runtime.

The MVP uses Chat Completions-compatible endpoints and requires typed JSON
responses. Automation scripts may read provider environment variables only to
seed an isolated temporary settings file; they do not turn those variables into
runtime overrides.

## MVP citation evaluation

The fixed evaluation set contains eight Markdown sources and ten questions. Each
answer must keep at least one citation whose path and quote are validated
against the current revision manifest. The default acceptance threshold is 80%.

Run the evaluation against a real OpenAI-compatible endpoint with:

```bash
OPENAI_API_KEY=... scripts/llm-evaluation.sh
```

The runner creates a temporary workspace, ingests every source in
`fixtures/mvp-eval/sources`, runs lint, asks all questions in
`fixtures/mvp-eval/questions.json`, and writes a JSON report to
`target/corpusbot-mvp-evaluation.json`. Set `KEEP_WORKSPACE=1` to retain the
workspace for debugging, `WORKSPACE` to reuse a path, `REPORT` to move the
report, and `MIN_PASS_RATE` to change the acceptance threshold.

## Desktop E2E

The E2E suite starts a real local backend and the Vite frontend. The backend is
launched with `CORPUSBOT_E2E_LLM=1`, which enables a deterministic fake provider;
it never calls a model provider. Playwright starts both services automatically.
For manual inspection while the suite is running, the UI is served at:

```text
http://127.0.0.1:1420
```

It provides a real workspace, storage, HTTP backend, and deterministic LLM
responses so Playwright can exercise the React interface without starting a
Tauri process or calling a real provider:

```bash
pnpm exec playwright install chromium
pnpm test:e2e
```

The E2E flow opens a workspace, reads a page, imports Markdown, asks a cited
question, checks Documents and the Lint report, and restores a snapshot.

## Current MVP limitations

- Ingest is serial and Markdown-only; PDF/EPUB/HTML import is not implemented.
- Ingest job status is process-local; queued or running jobs are not resumed
  after an app/server restart.
- Ingest rebuilds the Tantivy generation rather than performing segment-level incremental updates.
- Snapshot restore immediately rebuilds the Tantivy generation rather than performing segment-level incremental updates.
- Interrupted Ingest and Restore runs reconcile automatically on the next open; broader fault-injection hardening remains future work.
- Lint reports issues but does not auto-repair.
- MCP, graph visualization, clustering, and Deep Research are intentionally post-MVP.

## Quality checks

```bash
make check
```

This runs Rust formatting and Clippy, Rust and frontend tests, the TypeScript check, and the production frontend build. `make fmt` formats both Rust and frontend sources.

`make check` also runs the offline MVP CLI acceptance flow. To run it directly,
use:

```bash
scripts/mvp-acceptance.sh
```

The script uses a deterministic local OpenAI-compatible provider, exercises all
eight CLI commands, verifies a path/quote/revision citation, and writes
`target/mvp-acceptance.json`.

Continuous Integration runs the same checks on every pull request. To validate an OpenAI-compatible endpoint end to end, run:

```bash
OPENAI_API_KEY=... scripts/llm-smoke.sh
```

The smoke script creates a temporary workspace, ingests the sample source, runs lint, and asks a real question. Override `SOURCE_FILE`, `QUESTION`, or `WORKSPACE` to exercise your own material. Set `KEEP_WORKSPACE=1` to retain the workspace and print its path for debugging.
