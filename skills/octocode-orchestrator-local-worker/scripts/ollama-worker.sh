#!/usr/bin/env bash
# Non-interactive Ollama worker invoke for octocode-orchestrator-local-worker.
# Usage:
#   ./scripts/ollama-worker.sh --model NAME --job summarize --input file.txt [--schema hint.txt] [--out out.json]
#   ./scripts/ollama-worker.sh --model NAME --job vision --image file.png [--schema hint.txt] [--out out.json]
#   ./scripts/ollama-worker.sh --model NAME --packet packet.txt [--out out.json] [--format-json]
set -euo pipefail

HOST="${OLLAMA_HOST:-http://127.0.0.1:11434}"
MODEL="${OLLAMA_WORKER_MODEL:-}"
JOB=""
INPUT_PATH=""
IMAGE_PATH=""
SCHEMA_PATH=""
PACKET_PATH=""
OUT_PATH=""
DRY_RUN=0
FORMAT_JSON=0
THINK_FLAG=""
HIDE_THINKING=0

usage() {
  cat <<'EOF'
Usage:
  ollama-worker.sh --model <exact-name> --job <summarize|extract|classify|draft|map|check|vision|translate> --input <file> [--schema <file>] [--out <file>]
  ollama-worker.sh --model <exact-name> --job vision --image <png|jpg> [--schema <file>] [--out <file>]
  ollama-worker.sh --model <exact-name> --packet <file> [--out <file>]

Env:
  OLLAMA_HOST           default http://127.0.0.1:11434
  OLLAMA_WORKER_MODEL   used if --model omitted

Options:
  --format-json       Pass --format json to ollama run
  --think true|false   Pass --think=VALUE (also accepts --think=false)
  --think=true|false   Same as --think (equals form)
  --hidethinking      Pass --hidethinking
  --image <path>      Image for vision jobs (understand/caption — not generation)
  --dry-run           Print the prompt; do not call Ollama
  -h, --help          Show help

Model MUST be an exact name from `ollama list`.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --model) MODEL="${2:-}"; shift 2 ;;
    --job) JOB="${2:-}"; shift 2 ;;
    --input) INPUT_PATH="${2:-}"; shift 2 ;;
    --image) IMAGE_PATH="${2:-}"; shift 2 ;;
    --schema) SCHEMA_PATH="${2:-}"; shift 2 ;;
    --packet) PACKET_PATH="${2:-}"; shift 2 ;;
    --out) OUT_PATH="${2:-}"; shift 2 ;;
    --format-json) FORMAT_JSON=1; shift ;;
    --think) THINK_FLAG="${2:-}"; shift 2 ;;
    --think=*) THINK_FLAG="${1#*=}"; shift ;;
    --hidethinking) HIDE_THINKING=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown arg: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "${MODEL}" ]]; then
  echo "error: --model or OLLAMA_WORKER_MODEL required (pick from ollama list)" >&2
  usage >&2
  exit 2
fi

WORKSPACE_OCTOCODE_DIR="$(node -e 'const path=require("node:path"); process.stdout.write(path.resolve(process.cwd(), ".octocode"))')"
GLOBAL_OCTOCODE_DIR="$(node -e 'import("@octocodeai/config").then(m=>process.stdout.write(m.getOctocodeHome())).catch(e=>{console.error(e.message); process.exit(1);})')"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if ! "${SCRIPT_DIR}/ollama-health.sh" --model "${MODEL}"; then
  exit 1
fi

build_prompt_from_parts() {
  local input_body schema_body
  if [[ -n "${INPUT_PATH}" ]]; then
    input_body="$(cat "${INPUT_PATH}")"
  else
    input_body="(image input attached via --image)"
  fi
  if [[ -n "${SCHEMA_PATH}" ]]; then
    schema_body="$(cat "${SCHEMA_PATH}")"
  else
    schema_body='Return concise structured text. Prefer JSON if the job implies fields.'
  fi
  cat <<EOF
You are a local worker. Complete only the JOB. Obey CONSTRAINTS.
Return ONLY the result. No markdown fences unless the schema requires them.

GOAL: Perform job '${JOB}' on the provided INPUT.
JOB: ${JOB}
MODEL: ${MODEL}

CONSTRAINTS:
- no tools, no shell, no web
- do not invent files, APIs, or line numbers
- do not generate or invent images — vision jobs only describe provided images
- if unsure, emit null / "unknown" and say why

OUTPUT_SCHEMA:
${schema_body}

INPUT:
${input_body}
EOF
}

PROMPT_DIR="${WORKSPACE_OCTOCODE_DIR}/tmp/ollama-worker"
mkdir -p "${PROMPT_DIR}"
PROMPT_FILE="${PROMPT_DIR}/prompt-$$.txt"
cleanup() { rm -f "${PROMPT_FILE}"; }
trap cleanup EXIT

if [[ -n "${PACKET_PATH}" ]]; then
  [[ -f "${PACKET_PATH}" ]] || { echo "error: packet not found: ${PACKET_PATH}" >&2; exit 2; }
  cat "${PACKET_PATH}" >"${PROMPT_FILE}"
else
  [[ -n "${JOB}" ]] || { echo "error: --job or --packet required" >&2; usage >&2; exit 2; }
  case "${JOB}" in
    summarize|extract|classify|draft|map|check|vision|translate) ;;
    *) echo "error: unsupported job '${JOB}'" >&2; exit 2 ;;
  esac
  if [[ "${JOB}" == "vision" ]]; then
    [[ -n "${IMAGE_PATH}" && -f "${IMAGE_PATH}" ]] || { echo "error: --image file required for vision" >&2; exit 2; }
  else
    [[ -n "${INPUT_PATH}" && -f "${INPUT_PATH}" ]] || { echo "error: --input file required" >&2; exit 2; }
  fi
  build_prompt_from_parts >"${PROMPT_FILE}"
fi

if [[ -n "${IMAGE_PATH}" && ! -f "${IMAGE_PATH}" ]]; then
  echo "error: image not found: ${IMAGE_PATH}" >&2
  exit 2
fi

if [[ "${DRY_RUN}" -eq 1 ]]; then
  echo "model: ${MODEL}" >&2
  [[ -n "${IMAGE_PATH}" ]] && echo "image: ${IMAGE_PATH}" >&2
  cat "${PROMPT_FILE}"
  exit 0
fi

if ! command -v ollama >/dev/null 2>&1; then
  echo "error: ollama CLI not on PATH" >&2
  exit 2
fi

RUN_ARGS=(run)
if [[ "${FORMAT_JSON}" -eq 1 ]]; then
  RUN_ARGS+=(--format json)
fi
if [[ -n "${THINK_FLAG}" ]]; then
  RUN_ARGS+=("--think=${THINK_FLAG}")
fi
if [[ "${HIDE_THINKING}" -eq 1 ]]; then
  RUN_ARGS+=(--hidethinking)
fi
RUN_ARGS+=("${MODEL}")
if [[ -n "${IMAGE_PATH}" ]]; then
  RUN_ARGS+=("${IMAGE_PATH}")
fi

RESULT="$(ollama "${RUN_ARGS[@]}" <"${PROMPT_FILE}")"

if [[ -n "${OUT_PATH}" ]]; then
  ABS_OUT="$(node -e 'const path=require("node:path"); process.stdout.write(path.resolve(process.argv[1]))' "${OUT_PATH}")"
  case "${ABS_OUT}" in
    "${WORKSPACE_OCTOCODE_DIR}"/*|"${GLOBAL_OCTOCODE_DIR}"/*) ;;
    *) echo "error: --out must be under ${WORKSPACE_OCTOCODE_DIR} or ${GLOBAL_OCTOCODE_DIR}" >&2; exit 2 ;;
  esac
  mkdir -p "$(dirname "${ABS_OUT}")"
  printf '%s\n' "${RESULT}" >"${ABS_OUT}"
  echo "wrote ${ABS_OUT} (model=${MODEL})" >&2
else
  printf '%s\n' "${RESULT}"
fi
