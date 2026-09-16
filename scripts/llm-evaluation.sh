#!/usr/bin/env bash
set -euo pipefail

: "${OPENAI_API_KEY:?OPENAI_API_KEY must be set}"
: "${SOURCE_GLOB:=fixtures/mvp-eval/sources/*.md}"
: "${QUESTIONS:=fixtures/mvp-eval/questions.json}"
: "${REPORT:=target/corpusbot-mvp-evaluation.json}"
: "${MIN_PASS_RATE:=0.8}"
: "${WORKSPACE:=$(mktemp -d)}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACTS="$(mktemp -d)"

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

cargo build -q -p corpusbot-cli
CLI="$ROOT/target/debug/corpusbot"

# Environment values seed an isolated config; they do not override app settings.
CONFIG_HOME="$ARTIFACTS/config-home"
mkdir -p "$CONFIG_HOME/config/CorpusBot" "$CONFIG_HOME/Library/Application Support/CorpusBot"
for config_root in "$CONFIG_HOME/config/CorpusBot" "$CONFIG_HOME/Library/Application Support/CorpusBot"; do
  python3 - "$config_root/settings.json" <<'PY'
import json
import os
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
settings = {
    "base_url": os.environ.get("OPENAI_BASE_URL", "https://api.openai.com/v1"),
    "model": os.environ.get("CORPUSBOT_MODEL", "gpt-4o-mini"),
    "api_key": os.environ["OPENAI_API_KEY"],
}
path.write_text(json.dumps(settings, indent=2) + "\n")
path.chmod(0o600)
PY
done
export HOME="$CONFIG_HOME"
export XDG_CONFIG_HOME="$CONFIG_HOME/config"
unset OPENAI_API_KEY OPENAI_BASE_URL CORPUSBOT_MODEL

for source in "${sources[@]}"; do
  echo "ingesting $source" >&2
"$CLI" ingest --root "$WORKSPACE" --file "$source" >/dev/null
done

"$CLI" lint --root "$WORKSPACE" --format table
"$CLI" evaluate \
  --root "$WORKSPACE" \
  --questions "$QUESTIONS" \
  --min-pass-rate "$MIN_PASS_RATE" \
  --output "$REPORT"

echo "report: $REPORT" >&2
