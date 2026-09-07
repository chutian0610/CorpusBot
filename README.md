# CorpusBot

CorpusBot is a local-first knowledge-base engine that compiles Markdown sources into a maintained Wiki and answers questions with verifiable citations.

The MVP implementation plan is in [docs/MVP_PLAN.md](docs/MVP_PLAN.md). Architecture decisions are in [docs/adr](docs/adr).

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

## Quality checks

```bash
make check
```

This runs Rust formatting and Clippy, Rust and frontend tests, the TypeScript check, and the production frontend build. `make fmt` formats both Rust and frontend sources.
