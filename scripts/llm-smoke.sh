#!/usr/bin/env bash
set -euo pipefail

: "${OPENAI_API_KEY:?OPENAI_API_KEY must be set}"
: "${SOURCE_FILE:=fixtures/sample-source.md}"
: "${QUESTION:=What is the source about?}"
: "${WORKSPACE:=$(mktemp -d)}"

cleanup() {
  if [[ "${KEEP_WORKSPACE:-0}" != "1" ]]; then
    rm -rf "$WORKSPACE"
  else
    echo "workspace: $WORKSPACE"
  fi
}
trap cleanup EXIT

cargo run -q -p corpusbot-cli -- init --root "$WORKSPACE" --template research
cargo run -q -p corpusbot-cli -- ingest --root "$WORKSPACE" --file "$SOURCE_FILE"
cargo run -q -p corpusbot-cli -- lint --root "$WORKSPACE" --format json
cargo run -q -p corpusbot-cli -- query --root "$WORKSPACE" --question "$QUESTION"
