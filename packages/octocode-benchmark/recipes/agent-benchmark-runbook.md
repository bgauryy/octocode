# Agent Benchmark Runbook

Use this before running any Octocode benchmark recipe. The goal is to make every
run deterministic enough to compare, easy to audit, and honest about evidence
quality.

All benchmark output for a run must live under:

```text
packages/octocode-benchmark/output/<benchmark-name>-<YYYYMMDDTHHMMSSZ>/
```

Example:

```bash
BENCHMARK_NAME="octocode-live-tools"
BENCH_TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
BENCH_OUT="packages/octocode-benchmark/output/${BENCHMARK_NAME}-${BENCH_TIMESTAMP}"
RAW="$BENCH_OUT/raw"
mkdir -p "$RAW" "$BENCH_OUT/artifacts" "$BENCH_OUT/schemes"
export BENCHMARK_NAME BENCH_TIMESTAMP BENCH_OUT RAW
```

Never write benchmark artifacts to `/tmp` for a reported run. `/tmp` is only for
throwaway local debugging before the run starts.

---

## Required Output Layout

Every completed run writes these files:

```text
output/<benchmark-name>-<YYYYMMDDTHHMMSSZ>/
  README.md              # human index: what was run, verdict, how to reproduce
  manifest.json          # environment, git state, fixed inputs, cache mode
  summary.json           # machine-readable result, validates against output-run.schema.json
  commands.ndjson        # one command record per line
  results.md             # human-readable measurements and evidence anchors
  reflection.md          # what worked, what did not, missing pieces, improvements, praise
  ratings.json           # 1-10 scores with reasons
  raw/                   # raw stdout/stderr/tool JSON, one file per command
  schemes/               # schemas/help captured before running
  artifacts/             # optional derived tables, screenshots, corpus manifests
```

The canonical schema for `summary.json` is
[output-run.schema.json](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/output-run.schema.json).

---

## Determinism Rules

1. Build first: `yarn build`, or state why the benchmark intentionally uses an
   already-built artifact.
2. Record `git rev-parse HEAD` and `git status --short`. Dirty worktrees are
   allowed, but must be reported in `manifest.json`.
3. Capture schemas before running commands:
   `octocode --help --compact`, `octocode tools --compact`,
   `octocode search --scheme --compact`, and every raw tool scheme touched by the
   benchmark.
4. Fix all input bounds: `limit`, `page`, `itemsPerPage`, line ranges, paths,
   refs, package names, and file names.
5. Pin external sources when possible. For GitHub, record resolved branch/ref and
   whether the result came from cache. For npm, record package name and returned
   version.
6. Separate **cold-cache** from **warm-cache** timings. Do not mix them into one
   score.
7. Treat live GitHub/npm provider results as time-sensitive. Record auth state
   and rate-limit or anonymous mode if visible.
8. Save raw command output under `$RAW`. Do not summarize from terminal memory.
9. Redact secrets before writing environment data. Record only env key presence,
   never token values.
10. Evidence grade matters: candidate-grade rows are valid benchmark outputs, but
    must not be scored as proof until followed by exact fetch, LSP, or graph proof.

Recommended measurement:

```bash
CLI_CMD="node packages/octocode/out/octocode.js"
CLI=(node packages/octocode/out/octocode.js)
start_ms="$(node -e 'console.log(Date.now())')"
"${CLI[@]}" search --scheme --compact >"$RAW/octocode-search-scheme.json" 2>"$RAW/octocode-search-scheme.stderr"
exit_code="$?"
end_ms="$(node -e 'console.log(Date.now())')"
duration_ms="$((end_ms - start_ms))"
printf '{"id":"search-scheme","command":"%s","exitCode":%s,"durationMs":%s,"stdoutFile":"%s","stderrFile":"%s"}\n' \
  "$CLI_CMD search --scheme --compact" "$exit_code" "$duration_ms" \
  "$RAW/octocode-search-scheme.json" "$RAW/octocode-search-scheme.stderr" \
  >>"$BENCH_OUT/commands.ndjson"
```

For short local benchmarks, run each command at least 3 times after one warmup
and report median. For live provider benchmarks, one run is acceptable if it is
marked `networkUsed:true` and `cacheMode` is accurate.

---

## Manifest Template

`manifest.json` records how to reproduce the run:

```json
{
  "schemaVersion": "1.0.0",
  "benchmarkName": "octocode-live-tools",
  "timestampUtc": "2026-06-23T08:30:00Z",
  "outputDir": "packages/octocode-benchmark/output/octocode-live-tools-20260623T083000Z",
  "gitCommit": "abcdef0",
  "gitDirty": true,
  "node": "v26.3.1",
  "packageManager": "yarn",
  "cliCommand": "node packages/octocode/out/octocode.js",
  "authState": "anonymous",
  "cacheMode": "warm",
  "fixedInputs": [
    "repo=pmndrs/zustand",
    "path=src/vanilla.ts",
    "symbol=createStore",
    "package=zod"
  ]
}
```

---

## Result Summary Template

`summary.json` must validate against the schema. Minimal shape:

```json
{
  "schemaVersion": "1.0.0",
  "benchmark": {
    "name": "octocode-live-tools",
    "kind": "cli-tools",
    "recipe": "packages/octocode-benchmark/benchmark/octocode/README.md"
  },
  "run": {
    "runId": "octocode-live-tools-20260623T083000Z",
    "timestampUtc": "2026-06-23T08:30:00Z",
    "outputDir": "packages/octocode-benchmark/output/octocode-live-tools-20260623T083000Z",
    "gitCommit": "abcdef0",
    "gitDirty": true
  },
  "environment": {
    "os": "darwin",
    "arch": "arm64",
    "node": "v26.3.1",
    "packageManager": "yarn",
    "cliCommand": "node packages/octocode/out/octocode.js",
    "authState": "anonymous"
  },
  "determinism": {
    "cacheMode": "warm",
    "fixedInputs": ["pmndrs/zustand", "src/vanilla.ts", "createStore", "zod"],
    "paginationFixed": true,
    "networkUsed": true,
    "knownNondeterminism": ["GitHub provider latency"]
  },
  "surfaces": [
    {"name": "raw/MCP tools", "expected": 14, "observed": 14, "status": "pass"}
  ],
  "flows": [
    {
      "name": "remote-as-local",
      "commands": [
        {
          "id": "remote-ls",
          "command": "octocode search src --repo pmndrs/zustand --tree --json --compact",
          "exitCode": 0,
          "durationMs": 421,
          "stdoutFile": "raw/octocode-remote-local-ls.json"
        }
      ],
      "status": "pass",
      "evidence": {"kind": "proof", "answerReady": true, "complete": true},
      "anchors": ["pmndrs/zustand", "src/vanilla.ts", "createStore"],
      "bestContinuation": "localPath from location",
      "outputQualityScore": 5
    }
  ],
  "ratings": {
    "rawMcpTools": {"score": 9, "reason": "Schemas are complete and runnable."},
    "oqlSearch": {"score": 9, "reason": "Targets expose evidence and continuations."},
    "quickCli": {"score": 8, "reason": "Good shortcuts; envelopes differ by command."},
    "flowQuality": {"score": 9, "reason": "Search-to-proof path is clear."},
    "schemaQuality": {"score": 9, "reason": "No stale fields observed."},
    "dataQuality": {"score": 8, "reason": "Provider rows need follow-up proof."},
    "researchQuality": {"score": 8, "reason": "Nested research emits next.graph."},
    "outputQuality": {"score": 9, "reason": "Raw and reflection artifacts are complete."}
  },
  "reflection": {
    "whatWorked": ["Remote-as-local returned local proof anchors."],
    "whatDidNotWork": [],
    "missing": [],
    "possibleImprovements": ["Automate commands.ndjson generation."],
    "praises": ["The continuation model made proof steps explicit."],
    "nextFix": "Turn this recipe into a runner."
  },
  "verdict": {
    "status": "pass",
    "summary": "All benchmark surfaces met the required thresholds."
  }
}
```

---

## Reflection Template

`reflection.md` is not optional. Use this exact section order:

```md
# Reflection

## What Worked

- Fast exact anchors:
- Copy-paste runnable continuations:
- Best tool/flow:

## What Did Not Work

- Failures:
- Friction:
- Misleading or noisy output:

## Missing

- Missing benchmark command:
- Missing schema/help detail:
- Missing proof continuation:

## Possible Improvements

- Product/code improvement:
- Benchmark automation improvement:
- Documentation improvement:

## Praises

- Strongest tool behavior:
- Best output-quality moment:
- Most agent-friendly continuation:

## Ratings

| Surface | Score | Reason |
|---|---:|---|
| Raw/MCP tools |  |  |
| OQL search |  |  |
| Quick CLI commands |  |  |
| Remote-as-local |  |  |
| Data quality |  |  |
| Schema quality |  |  |
| Research quality |  |  |
| Output quality |  |  |

## Next Fix

- 
```

Praise is required because it captures what should be preserved, not just what
should be fixed.

---

## Quality Gates

A run can be marked `pass` only when:

- All commands required by the recipe either pass or are marked skipped with a
  deterministic reason.
- `summary.json` validates against
  [output-run.schema.json](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/output-run.schema.json).
- `reflection.md` includes what worked, what did not, missing items, possible
  improvements, praises, ratings, and one next fix.
- Candidate-grade research/provider rows are not counted as proof unless the run
  follows exact fetch, LSP, graph proof, or materialized local proof.
- Raw outputs are saved under `$RAW`; no reported result depends on `/tmp`.

Use `warn` when behavior works but output quality, determinism, or schema clarity
is below target. Use `fail` when a required route is broken, stale, or missing a
repair path.
