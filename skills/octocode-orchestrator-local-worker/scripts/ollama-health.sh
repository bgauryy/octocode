#!/usr/bin/env bash
# Health-check local Ollama for octocode-orchestrator-local-worker.
# Usage:
#   ./scripts/ollama-health.sh              # daemon up
#   ./scripts/ollama-health.sh --model NAME  # daemon + exact model present
#   ./scripts/ollama-health.sh --list        # print installed model names
set -euo pipefail

HOST="${OLLAMA_HOST:-http://127.0.0.1:11434}"
MODEL=""
LIST_ONLY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --model) MODEL="${2:-}"; shift 2 ;;
    --list) LIST_ONLY=1; shift ;;
    -h|--help)
      echo "Usage: ollama-health.sh [--model NAME] [--list]"
      exit 0
      ;;
    *) echo "error: unknown arg: $1" >&2; exit 2 ;;
  esac
done

if ! command -v curl >/dev/null 2>&1; then
  echo "error: curl is required" >&2
  exit 2
fi

if ! curl -sS --max-time 3 "${HOST}/api/tags" >/dev/null; then
  echo "error: Ollama not reachable at ${HOST}" >&2
  echo "hint: start with 'ollama serve' and pull a model" >&2
  exit 1
fi

TAGS="$(curl -sS --max-time 5 "${HOST}/api/tags")"

names_json() {
  if command -v jq >/dev/null 2>&1; then
    echo "${TAGS}" | jq -r '.models[]?.name // empty'
  else
    # best-effort without jq
    echo "${TAGS}" | tr ',' '\n' | sed -n 's/.*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p'
  fi
}

if [[ "${LIST_ONLY}" -eq 1 ]]; then
  names_json
  exit 0
fi

echo "ok: ollama up at ${HOST}"

if [[ -z "${MODEL}" ]]; then
  echo "hint: run 'ollama list' then select via references/model-selection.md (catalog/RAM: references/ollama-local-models.md only if needed)"
  exit 0
fi

# Exact match only (avoid llama3.2 matching llama3.2-vision)
if command -v jq >/dev/null 2>&1; then
  if ! echo "${TAGS}" | jq -e --arg m "${MODEL}" '
      .models // [] | map(.name) | index($m) != null
    ' >/dev/null; then
    echo "error: model '${MODEL}' not found (exact name required)" >&2
    echo "installed:" >&2
    names_json >&2 || true
    echo "hint: ollama pull ${MODEL}  (ask user first)" >&2
    exit 1
  fi
else
  if ! names_json | grep -Fxq "${MODEL}"; then
    echo "error: model '${MODEL}' not found (exact name required)" >&2
    names_json >&2 || true
    exit 1
  fi
fi

echo "ok: model '${MODEL}' present (exact match)"
exit 0
