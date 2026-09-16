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

# Run the CLI with an isolated settings file; environment variables cannot
# override CorpusBot settings at runtime.
CONFIG_HOME="$ARTIFACTS/config-home"
LINUX_CONFIG_DIR="$CONFIG_HOME/config/CorpusBot"
MAC_CONFIG_DIR="$CONFIG_HOME/Library/Application Support/CorpusBot"
mkdir -p "$LINUX_CONFIG_DIR" "$MAC_CONFIG_DIR"
for settings_file in "$LINUX_CONFIG_DIR/settings.json" "$MAC_CONFIG_DIR/settings.json"; do
  cat >"$settings_file" <<JSON
{
  "base_url": "http://127.0.0.1:$PORT/v1",
  "model": "mvp-mock",
  "api_key": "mvp-acceptance-local-key",
  "git_author_name": "MVP Acceptance",
  "git_author_email": "acceptance@corpusbot.invalid"
}
JSON
  chmod 600 "$settings_file"
done
export HOME="$CONFIG_HOME"
export XDG_CONFIG_HOME="$CONFIG_HOME/config"
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
