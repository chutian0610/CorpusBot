#!/usr/bin/env bash
set -euo pipefail

: "${OPENAI_API_KEY:?OPENAI_API_KEY must be set}"
: "${SOURCE_GLOB:=fixtures/mvp-eval/sources/*.md}"
: "${QUESTIONS:=fixtures/mvp-eval/questions.json}"
: "${REPORT:=target/corpusbot-mvp-evaluation.json}"
: "${MIN_PASS_RATE:=0.8}"
: "${WORKSPACE:=$(mktemp -d)}"

sources=($SOURCE_GLOB)
if (( ${#sources[@]} < 5 || ${#sources[@]} > 10 )); then
  echo "evaluation must contain 5-10 sources; found ${#sources[@]}" >&2
  exit 2
fi

cleanup() {
  if [[ "${KEEP_WORKSPACE:-0}" != "1" ]]; then
    rm -rf "$WORKSPACE"
  else
    echo "workspace: $WORKSPACE"
  fi
}
trap cleanup EXIT

cargo run -q -p corpusbot-cli -- init --root "$WORKSPACE" --template research

for source in "${sources[@]}"; do
  echo "ingesting $source" >&2
  cargo run -q -p corpusbot-cli -- ingest --root "$WORKSPACE" --file "$source" >/dev/null
done

cargo run -q -p corpusbot-cli -- lint --root "$WORKSPACE" --format table
cargo run -q -p corpusbot-cli -- evaluate \
  --root "$WORKSPACE" \
  --questions "$QUESTIONS" \
  --min-pass-rate "$MIN_PASS_RATE" \
  --output "$REPORT"

echo "report: $REPORT" >&2
