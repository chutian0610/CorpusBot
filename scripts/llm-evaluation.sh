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
CONFIG_DB="$ARTIFACTS/daemon.db"
export CORPUSBOT_DAEMON_DB="$CONFIG_DB"
mkdir -p "$(dirname "$CONFIG_DB")"
python3 - "$CONFIG_DB" <<'PY'
import os
import pathlib
import sqlite3
import sys

database = pathlib.Path(sys.argv[1])
database.parent.mkdir(parents=True, exist_ok=True)
connection = sqlite3.connect(database)
connection.execute(
    """
    CREATE TABLE IF NOT EXISTS app_settings (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      base_url TEXT,
      model TEXT,
      api_key TEXT,
      git_author_name TEXT,
      git_author_email TEXT,
      updated_at TEXT NOT NULL
    )
    """
)
connection.execute(
    """
    INSERT OR REPLACE INTO app_settings (
      id, base_url, model, api_key, git_author_name, git_author_email, updated_at
    ) VALUES (1, ?, ?, ?, NULL, NULL, datetime('now'))
    """,
    (
        os.environ.get("OPENAI_BASE_URL", "https://api.openai.com/v1"),
        os.environ.get("CORPUSBOT_MODEL", "gpt-4o-mini"),
        os.environ["OPENAI_API_KEY"],
    ),
)
connection.commit()
connection.close()
database.chmod(0o600)
PY
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
