#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SOURCE="${SOURCE:-$ROOT/fixtures/sample-source.md}"
WORKSPACE="${WORKSPACE:-$(mktemp -d -t corpusbot-mvp-XXXXXX)}"
REPORT="${REPORT:-$ROOT/target/mvp-acceptance.json}"
KEEP_WORKSPACE="${KEEP_WORKSPACE:-0}"
MOCK_VERBOSE="${MOCK_VERBOSE:-0}"

ARTIFACTS="$(mktemp -d)"
MOCK_PID=""

cleanup() {
  if [[ -n "$MOCK_PID" ]]; then
    kill "$MOCK_PID" 2>/dev/null || true
    wait "$MOCK_PID" 2>/dev/null || true
  fi
  rm -rf "$ARTIFACTS"
  if [[ "$KEEP_WORKSPACE" != "1" ]]; then
    rm -rf "$WORKSPACE"
  else
    echo "workspace: $WORKSPACE" >&2
  fi
}
trap cleanup EXIT

mkdir -p "$(dirname "$REPORT")"
PORT_FILE="$ARTIFACTS/port"
MOCK_ARGS=(--port-file "$PORT_FILE")
if [[ "$MOCK_VERBOSE" == "1" ]]; then
  MOCK_ARGS+=(--verbose)
fi
python3 "$ROOT/scripts/mvp_mock_provider.py" "${MOCK_ARGS[@]}" &
MOCK_PID=$!

for _ in {1..50}; do
  [[ -s "$PORT_FILE" ]] && break
  sleep 0.1
done
if [[ ! -s "$PORT_FILE" ]]; then
  echo "mock provider failed to start" >&2
  exit 1
fi
PORT="$(cat "$PORT_FILE")"
cargo build -q -p corpusbot-cli
CLI="$ROOT/target/debug/corpusbot"

# Run the CLI with an isolated app-level SQLite database.
CONFIG_DB="$ARTIFACTS/daemon.db"
export CORPUSBOT_DAEMON_DB="$CONFIG_DB"
mkdir -p "$(dirname "$CONFIG_DB")"
python3 - "$CONFIG_DB" "$PORT" <<'PY'
import pathlib
import sqlite3
import sys

database = pathlib.Path(sys.argv[1])
port = sys.argv[2]
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
    ) VALUES (1, ?, ?, ?, ?, ?, datetime('now'))
    """,
    (
        f"http://127.0.0.1:{port}/v1",
        "mvp-mock",
        "mvp-acceptance-local-key",
        "MVP Acceptance",
        "acceptance@corpusbot.invalid",
    ),
)
connection.commit()
connection.close()
database.chmod(0o600)
PY
unset OPENAI_API_KEY OPENAI_BASE_URL CORPUSBOT_MODEL

json_value() {
  python3 -c 'import json, sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1"
}

run_step() {
  local name="$1"
  shift
  echo "==> $name" >&2
  set +e
  "$@" >"$ARTIFACTS/$name.json" 2>"$ARTIFACTS/$name.stderr"
  local status=$?
  set -e
  if [[ $status -ne 0 ]]; then
    cat "$ARTIFACTS/$name.stderr" >&2
    exit "$status"
  fi
}

run_step init "$CLI" init --root "$WORKSPACE" --template research
INIT_SNAPSHOT_ID="$(json_value headSnapshotId <"$ARTIFACTS/init.json")"

run_step status "$CLI" status --root "$WORKSPACE"
run_step ingest "$CLI" ingest --root "$WORKSPACE" --file "$SOURCE"
run_step query "$CLI" query --root "$WORKSPACE" --question "What elects a leader in Raft?"
run_step lint "$CLI" lint --root "$WORKSPACE" --format json
run_step snapshot "$CLI" snapshot --root "$WORKSPACE" --message "MVP acceptance snapshot"
run_step history "$CLI" history --root "$WORKSPACE" --limit 20

RESTORE_OUTPUT="$ARTIFACTS/restore.txt"
echo "==> restore" >&2
set +e
"$CLI" restore --root "$WORKSPACE" --snapshot "$INIT_SNAPSHOT_ID" --yes \
  >"$RESTORE_OUTPUT" 2>"$ARTIFACTS/restore.stderr"
restore_status=$?
set -e
if [[ $restore_status -ne 0 ]]; then
  cat "$ARTIFACTS/restore.stderr" >&2
  exit "$restore_status"
fi

run_step final-status "$CLI" status --root "$WORKSPACE"

python3 - "$REPORT" "$WORKSPACE" "$INIT_SNAPSHOT_ID" "$ARTIFACTS" <<'PY'
import json
import pathlib
import sys

report_path, workspace, init_snapshot_id, artifacts = (
    pathlib.Path(sys.argv[1]),
    sys.argv[2],
    sys.argv[3],
    pathlib.Path(sys.argv[4]),
)


def load(name: str) -> dict:
    return json.loads((artifacts / f"{name}.json").read_text())


init = load("init")
status = load("status")
ingest = load("ingest")
query = load("query")
lint = load("lint")
snapshot = load("snapshot")
history = load("history")
final_status = load("final-status")

assert init["root"] == workspace
assert len(init["headSnapshotId"]) == 40
assert status["page_count"] == 0
assert status["recovery_pending"] is False
assert ingest["status"] == "committed"
assert ingest["createdPaths"]
assert query["insufficientEvidence"] is False
assert query["citations"]
assert query["revisionManifestId"]
assert lint["summary"]["errors"] == 0
assert snapshot["result"] == "already_clean"
assert any(row["snapshotId"] == init_snapshot_id for row in history)
assert final_status["page_count"] == 0
assert final_status["recovery_pending"] is False

commands = [
    "init",
    "status",
    "ingest",
    "query",
    "lint",
    "snapshot",
    "history",
    "restore",
    "final status",
]
report = {
    "status": "passed",
    "workspace": workspace,
    "manifest_id": query["revisionManifestId"],
    "commands": commands,
    "citations": len(query["citations"]),
    "lint": lint["summary"],
    "history_entries": len(history),
}
report_path.write_text(json.dumps(report, indent=2) + "\n")
PY

echo "MVP CLI acceptance passed" >&2
echo "report: $REPORT" >&2
