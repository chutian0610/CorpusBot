#!/usr/bin/env bash
set -euo pipefail

: "${OPENAI_API_KEY:?OPENAI_API_KEY must be set}"
: "${SOURCE_FILE:=fixtures/sample-source.md}"
: "${QUESTION:=What is the source about?}"
: "${WORKSPACE:=$(mktemp -d)}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACTS="$(mktemp -d)"

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

"$CLI" init --root "$WORKSPACE" --template research
"$CLI" ingest --root "$WORKSPACE" --file "$SOURCE_FILE"
"$CLI" lint --root "$WORKSPACE" --format json
"$CLI" query --root "$WORKSPACE" --question "$QUESTION"
