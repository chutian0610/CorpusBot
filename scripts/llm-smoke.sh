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

"$CLI" init --root "$WORKSPACE" --template research
"$CLI" ingest --root "$WORKSPACE" --file "$SOURCE_FILE"
"$CLI" lint --root "$WORKSPACE" --format json
"$CLI" query --root "$WORKSPACE" --question "$QUESTION"
