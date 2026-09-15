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

The Vite dev server listens on <http://127.0.0.1:1420>. Run the desktop shell with:

```bash
pnpm tauri dev
```

## MVP workflow

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

The desktop Settings view writes an OpenAI-compatible configuration to the user-level CorpusBot config directory. The API key is stored privately and is not displayed again.

Environment variables take precedence over the saved settings:

- `OPENAI_API_KEY`
- `OPENAI_BASE_URL`
- `CORPUSBOT_MODEL`

The MVP uses Chat Completions-compatible endpoints and requires typed JSON responses.

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

## Desktop browser E2E

The desktop interface has a browser-only backend that is enabled only by opening:

```text
http://127.0.0.1:1420/?backend=browser
```

It provides deterministic workspace data so Playwright can exercise the React
interface without starting a Tauri process or calling a real provider:

```bash
pnpm exec playwright install chromium
pnpm test:e2e
```

The E2E flow opens a workspace, reads a page, imports Markdown, asks a cited
question, checks the Lint report, and restores a snapshot.

## Current MVP limitations

- Ingest is serial and Markdown-only; PDF/EPUB/HTML import is not implemented.
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
