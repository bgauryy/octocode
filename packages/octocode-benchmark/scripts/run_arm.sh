#!/usr/bin/env bash
# One isolated runner: a fresh headless `claude -p` for one (arm, pass, question). The arm
# decides which tools the agent gets; nothing else is loaded (no user/project settings,
# CLAUDE.md, skills, hooks, memory). The stream-json transcript is the measurement
# (transcript_to_log.py), identical for every arm.
#
#   run_arm.sh <campaign-dir> <arm> <pass> <Qn>
#
# Arms (brief in prompts/<arm>.md, shared brief prompts/runner.md):
#   mcp           Octocode MCP, clasify off       OCTO_MCP_CMD picks the server (default: local
#   mcp-clasify   Octocode MCP, clasify offered   build; e.g. "npx -y octocode-mcp@20.1.1")
#   mcp-<variant> Octocode MCP; clasify offered when the name contains "clasify".
#                 OCTO_MCP_CMD_<arm> (dashes as underscores) overrides OCTO_MCP_CMD
#   gh-lean       read-only gh + --json/--jq/--limit + text filters
#   gh            read-only gh, no filters
#   rtk           rtk gh …
#   headroom      ghc … (gh → Headroom compressor; needs HR_PY)
# Local arms (name starts with `local-`) answer local/datasets/core-v1 against the pinned
# clones in packages/octocode-benchmark/local/repos (LOCAL_REPOS) instead of the GitHub set:
#   local-shell       read-only Bash: rg, grep, find, ls, cat, sed -n, head, tail, wc, sort, uniq,
#                     cut, git (read-only via guard-local/git), ast-grep/sg
#   local-mcp[-<v>]   Octocode MCP started with the clones' root as cwd and WORKSPACE_ROOT;
#                     clasify offered when the name contains "clasify"; OCTO_MCP_CMD_<arm> as above
# Env: RUNNER_MODEL (default claude-sonnet-5-5), RUNNER_MAX_TURNS (default 60).
#      RUNNER=pi runs the same brief, arm tools and isolation with `pi` instead of `claude -p`
#      (pi_runner.py; PI_PROVIDER default guy-provider-anthropic-x, RUNNER_TIMEOUT default
#      1200 s). pi campaigns are not comparable with claude-runner campaigns.
set -euo pipefail

CAMP="$(mkdir -p "$1" && cd "$1" && pwd)"; ARM="$2"; PASS="$3"; Q="$4"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PKG/../.." && pwd)"
QDIR="external/questions"; RUNNER_BRIEF="runner.md"
if [ "${ARM#local-}" != "$ARM" ]; then
  QDIR="local/datasets/core-v1"; RUNNER_BRIEF="local-runner.md"
  LOCAL_REPOS="${LOCAL_REPOS:-$REPO/packages/octocode-benchmark/local/repos}"
fi
QFILE="$(python3 "$HERE/question_catalog.py" "${QUESTION_DIR:-$PKG/$QDIR}" "$Q" --resolve)"
Q="$(python3 "$HERE/question_catalog.py" "$(dirname "$QFILE")" "$Q")"
export BENCH_QUESTION_ID="$Q"
export QUESTION_REPO_MAP="${QUESTION_REPO_MAP:-$(dirname "$QFILE")/repositories.json}"
[ -z "${LOCAL_REPOS:-}" ] || export LOCAL_REPOS
[ -f "$QFILE" ] || { echo "missing $QFILE" >&2; exit 2; }
[ -f "$PKG/prompts/$ARM.md" ] || [ "${ARM#mcp}" != "$ARM" ] || [ "${ARM#local-mcp}" != "$ARM" ] \
  || { echo "unknown arm $ARM" >&2; exit 2; }

ID="$ARM-p$PASS-$Q"
RUN="$CAMP/runs/$ID"
WORK="$(mktemp -d -t octocode-bench-work)"  # fresh per question, outside evaluator storage
trap 'rmdir "$WORK" 2>/dev/null || true' EXIT
if [ -d "$RUN" ]; then
  ATTEMPTS="$CAMP/attempts/$ID"
  mkdir -p "$ATTEMPTS"
  COUNT="$(find "$ATTEMPTS" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
  [ "$((COUNT + 1))" -lt "${RUNNER_MAX_ATTEMPTS:-2}" ] || { echo "attempt cap reached: $ID" >&2; exit 2; }
  PREVIOUS="$ATTEMPTS/attempt-$(printf '%03d' "$((COUNT + 1))")"
  mv "$RUN" "$PREVIOUS"
  if [ -f "$CAMP/$ID.jsonl" ]; then
    mv "$CAMP/$ID.jsonl" "$PREVIOUS/measured.original.jsonl"
    python3 - "$PREVIOUS" "$RUN" <<'PYRELOCATE'
import json, sys
from pathlib import Path
old, previous = sys.argv[2], Path(sys.argv[1])
rows = []
for line in (previous / "measured.original.jsonl").read_text().splitlines():
    row = json.loads(line)
    if isinstance(row.get("artifact"), str) and row["artifact"].startswith(old + "/"):
        row["artifact"] = str(previous) + row["artifact"][len(old):]
    rows.append(row)
(previous / "measured.jsonl").write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rows))
PYRELOCATE
  fi
fi
mkdir -p "$RUN"

FLAGS=()
EXTRA_PATH=""
case "$ARM" in
  local-mcp|local-mcp-*)
    ARM_CMD_VAR="OCTO_MCP_CMD_${ARM//-/_}"
    read -r -a CMD <<< "${!ARM_CMD_VAR:-${OCTO_MCP_CMD:-node $REPO/packages/octocode-mcp/dist/index.js}}"
    # The server starts in the clones' root (cwd and WORKSPACE_ROOT), so relative paths resolve there.
    python3 - "$RUN/mcp.json" "$ARM" "$LOCAL_REPOS" "${CMD[@]}" <<'PY'
import json, sys
path, arm, root, cmd = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
env = {"NODE_ENV": "development", "WORKSPACE_ROOT": root, "OCTOCODE_BETA": "true", "OCTOCODE_RANKING_ENABLED": str("clasify" in arm).lower(),
       "TOOLS_TO_RUN": "localSearch,localFetch,structureSearch,astSearch,lspSearch" + (",clasify" if "clasify" in arm else "")}
if "clasify" not in arm:
    env["OCTOCODE_CLASSIFICATION_API"] = ""   # explicitly blank = clasify off
json.dump({"mcpServers": {"octocode": {"type": "stdio", "command": "sh",
                                       "args": ["-c", 'cd "$0" && exec "$@"', root, *cmd], "env": env}}},
          open(path, "w"), indent=1)
PY
    BRIEF="$PKG/prompts/mcp.md"
    FLAGS=(--tools "" --mcp-config "$RUN/mcp.json" --strict-mcp-config --allowedTools "mcp__octocode") ;;
  local-shell)
    BRIEF="$PKG/prompts/local-shell.md"
    export REAL_GIT="${REAL_GIT:-$(command -v git)}" REAL_FIND="${REAL_FIND:-$(command -v find)}"
    EXTRA_PATH="$HERE/guard-local"
    FLAGS=(--tools Bash --strict-mcp-config --add-dir "$LOCAL_REPOS" \
           --allowedTools "Bash(rg:*)" "Bash(grep:*)" "Bash(find:*)" "Bash(ls:*)" "Bash(cat:*)" "Bash(sed -n:*)" \
           "Bash(head:*)" "Bash(tail:*)" "Bash(wc:*)" "Bash(sort:*)" "Bash(uniq:*)" "Bash(cut:*)" \
           "Bash(git:*)" "Bash(ast-grep:*)" "Bash(sg:*)") ;;
  mcp|mcp-*)
    ARM_CMD_VAR="OCTO_MCP_CMD_${ARM//-/_}"
    read -r -a CMD <<< "${!ARM_CMD_VAR:-${OCTO_MCP_CMD:-node $REPO/packages/octocode-mcp/dist/index.js}}"
    python3 - "$RUN/mcp.json" "$ARM" "${CMD[@]}" <<'PY'
import json, sys
path, arm, cmd = sys.argv[1], sys.argv[2], sys.argv[3:]
env = {"NODE_ENV": "development", "OCTOCODE_BETA": "true", "OCTOCODE_RANKING_ENABLED": str("clasify" in arm).lower()}
if "clasify" not in arm:
    env["OCTOCODE_CLASSIFICATION_API"] = ""   # explicitly blank = clasify off
json.dump({"mcpServers": {"octocode": {"type": "stdio", "command": cmd[0], "args": cmd[1:], "env": env}}},
          open(path, "w"), indent=1)
PY
    BRIEF="$PKG/prompts/mcp.md"
    FLAGS=(--tools "" --mcp-config "$RUN/mcp.json" --strict-mcp-config --allowedTools "mcp__octocode") ;;
  gh-lean)
    BRIEF="$PKG/prompts/gh-lean.md"
    FLAGS=(--tools Bash --strict-mcp-config --allowedTools "Bash(gh:*)" "Bash(grep:*)" "Bash(sed -n:*)" "Bash(head:*)" \
           "Bash(tail:*)" "Bash(wc:*)" "Bash(sort:*)" "Bash(uniq:*)" "Bash(cut:*)" "Bash(jq:*)") ;;
  gh)
    BRIEF="$PKG/prompts/gh.md"
    FLAGS=(--tools Bash --strict-mcp-config --allowedTools "Bash(gh:*)") ;;
  rtk)
    BRIEF="$PKG/prompts/rtk.md"
    FLAGS=(--tools Bash --strict-mcp-config --allowedTools "Bash(rtk gh:*)") ;;
  headroom)
    BRIEF="$PKG/prompts/headroom.md"
    FLAGS=(--tools Bash --strict-mcp-config --allowedTools "Bash(ghc:*)") ;;
esac

PROMPT="$(cat "$PKG/prompts/$RUNNER_BRIEF")

$(cat "$BRIEF")

---

$(cat "$QFILE")"

# gh, rtk and ghc all resolve `gh` through the read-only guard.
export REAL_GH="${REAL_GH:-$(command -v gh)}"
export PATH="$HERE/guard:$HERE:$PATH"
[ -z "$EXTRA_PATH" ] || export PATH="$EXTRA_PATH:$PATH"   # local-shell: read-only git/find first
export GHC_LOG="$RUN/ghc.jsonl"   # headroom wrapper's own log (diagnostic only)

cd "$WORK"
if [ "${RUNNER:-claude}" = codex ]; then
  [ -f "$RUN/mcp.json" ] || { echo "Codex runner supports MCP-only arms" >&2; exit 2; }
  printf '%s' "$PROMPT" > "$RUN/prompt.md"
  set +e
  python3 "$HERE/codex_runner.py" --run "$RUN" --prompt "$RUN/prompt.md" \
    --mcp-config "$RUN/mcp.json" --model "${RUNNER_MODEL:-gpt-5.6-terra}" \
    --timeout "${RUNNER_TIMEOUT:-600}" --max-tool-calls "${RUNNER_MAX_TOOL_CALLS:-30}"
  STATUS=$?
  set -e
  python3 "$HERE/transcript_to_log.py" --format codex \
    --transcript "$RUN/transcript.jsonl" --log "$CAMP/$ID.jsonl" --run-dir "$RUN" --question "$Q" | sed "s/^/$ID /"
  exit "$STATUS"
fi
if [ "${RUNNER:-claude}" = pi ]; then
  printf '%s' "$PROMPT" > "$RUN/prompt.md"
  set +e
  python3 "$HERE/pi_runner.py" --run "$RUN" --prompt "$RUN/prompt.md" \
    --provider "${PI_PROVIDER:-guy-provider-anthropic-x}" --model "${RUNNER_MODEL:-claude-sonnet-5-5}" \
    --timeout "${RUNNER_TIMEOUT:-1200}" -- "${FLAGS[@]}"
  STATUS=$?
  set -e
  python3 "$HERE/transcript_to_log.py" --format pi --max-turns "${RUNNER_MAX_TURNS:-60}" \
    --transcript "$RUN/transcript.jsonl" --log "$CAMP/$ID.jsonl" --run-dir "$RUN" --question "$Q" | sed "s/^/$ID /"
  exit "$STATUS"
fi
set +e
claude -p "$PROMPT" \
  --model "${RUNNER_MODEL:-claude-sonnet-5-5}" \
  "${FLAGS[@]}" \
  --setting-sources "" \
  --disable-slash-commands \
  --no-session-persistence \
  --max-turns "${RUNNER_MAX_TURNS:-60}" \
  --output-format stream-json --verbose \
  > "$RUN/transcript.jsonl" 2> "$RUN/stderr.log"
STATUS=$?
set -e

python3 "$HERE/transcript_to_log.py" --transcript "$RUN/transcript.jsonl" \
  --log "$CAMP/$ID.jsonl" --run-dir "$RUN" --question "$Q" | sed "s/^/$ID /"
exit "$STATUS"
