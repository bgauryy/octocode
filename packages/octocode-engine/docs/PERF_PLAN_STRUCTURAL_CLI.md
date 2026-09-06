# Perf plan — closing the structural-search gap (post improvement-run-1)

Research date: 2026-08-02. Measured on the local build (branch `update-tools`), Apple Silicon, release engine. Method: stage-isolated timing via direct napi calls + `--cpu-prof` on the CLI query path. Raw numbers below are reproducible with the commands in §5.

Status update (2026-09-05): the headline comparison below is historical. A
five-run median on the rebuilt `updates` working tree found 8,469 identical
matches for every implementation: ast-grep CLI 33.90 ms, Octocode native 61.95
ms, and Octocode CLI 282.48 ms. The native matcher did not retain the reported
2x lead on this fixture. Re-run the sensor in section 5 before using this plan
to accept a performance change.

## 1. What the research found (kills the old theory)

The prior attribution — "remaining 3–19× vs ast-grep comes from no-prefilter full parse + `$$$` backtracking allocations" — is **wrong** post-de-ranking:

| Stage (engine/src Rust tree, 7,578 matches) | Measured |
|---|---|
| Engine `structuralSearchFiles`, metavar pattern `$FN($$$ARGS)` | **52 ms** (direct napi call) |
| Engine, `kind: call_expression` rule (no metavar machinery) | **20 ms** |
| ast-grep CLI end-to-end, same query | 110 ms |
| CLI end-to-end, count mode | 400 ms |
| CLI end-to-end, content mode | 2,250 ms |
| CLI on a **1-file directory** (floor) | **368 ms** |

On the 2026-08-02 fixture, the engine measured **2× faster than ast-grep**. The `$$$` backtracking (`CaptureEnv` clones) cost about 32 ms (52−20). The CLI gap in that run came from work outside the matcher:

1. **~236 ms/invocation: `sanitizeContent` first-call regex compile.** CPU profile: `nativeSanitizeContent` = 235.6 ms of a 359 ms query run. Root cause: `security/patterns.rs:1260` `REGEX_SET: LazyLock<RegexSet>` + `patterns.rs:1579` `PATTERN_REGEXES: LazyLock<Vec<Regex>>` — 309 secret patterns compiled lazily on first sanitize. Isolated: first call **800 ms** cold in a bare Node process, then 0.004 ms/call; a 300 KB clean scan is 0.6 ms. The MCP server pays this once per session — the CLI pays it **every invocation** (agents make 20–45 calls per task → 5–15 s of pure compile tax per task).
2. **~85 ms: CLI boot to dispatch** (module execution; V8 compile cache measured ≈ no help).
3. **~1.8 s content-mode TS assembly** at 7.5 k matches: per-match `text.replace(/\s+/g,…)` + slice in `structuralSearch.ts`, full match payload across napi, `cleanJsonObject` deep-walk, serialization.

## 2. Plan (priority = impact ÷ risk)

### P1 — Eliminate the sanitizer compile tax (~236–800 ms → ~0) — DO FIRST
The scan is cheap; only the compile is expensive, and almost all content is clean.
- **P1a (recommended): literal pregate + lazy tiers.** Extract literal anchors from the 309 patterns (secret formats are literal-rich: `AKIA`, `ghp_`, `xoxb-`, `-----BEGIN`, `eyJ`…). Build an aho-corasick automaton over the literals (sub-ms). Run the RegexSet/per-pattern regexes **only when a literal hits**, compiling them at that moment. Patterns with no extractable literal go in a small always-on subset. Clean content never compiles the heavy set.
- **P1b (if 1a leaves a tail): prebuilt DFA.** `regex-automata` supports build-time DFA serialization + zero-copy deserialize (`include_bytes!`). More invasive; only if needed.
- **P1c (stopgap, 1 line-ish): compile in a background thread at process start** so it overlaps the search. Hides latency, still burns CPU; use only as a bridge.
- Also make `PATTERN_REGEXES` compile **per-index on demand** (only the patterns whose RegexSet member matched), not all 309 at once.
- **Non-negotiable guardrail:** a red test seeding a fake AWS key / GitHub token / PEM block through each tier — masking behavior must be byte-identical before/after. Speed must not weaken sanitization.

### P2 — Content-mode assembly (2.25 s → target < 0.9 s at 7.5 k matches)
- Push `maxMatchesPerFile` + `matchContentLength` into `StructuralSearchFilesOptions` so Rust truncates **before** the napi crossing.
- Engine light mode for `output:"countMatches"/"files"`: skip per-match text + metavarRanges assembly (`to_structural_match_with_index`) — kind-rule timing shows ~30 ms of the 52 ms is assembly.
- Single-pass whitespace collapse in `structuralSearch.ts:210` (skip entirely for single-line matches) instead of per-match regex.
- `cleanJsonObject`: skip deep-walk for large `files[]` arrays of known-clean rows.

### P3 — Matcher micro-opts (52 → ~30 ms; LOW priority, only after P1+P2 re-measure)
- Undo-log (push/pop journal) instead of `CaptureEnv.clone()` per branch in `match_child_list` (octo.rs:694) / per `take` in `match_multi_capture` (octo.rs:748).
- Worth ~20 ms on a 7.5 k-match tree; do it only if post-P1/P2 profiles still show it.

### P4 — Not doing
- Text prefilter for anchorless patterns: engine already beats ast-grep without it.
- V8 compile cache / snapshot: measured ≈ no effect (cost is module execution + Rust compile, not JS parse).

## 3. KPI contract

- **Primary (CLI wall-clock, same machine, median of 3):** tiny-dir floor 0.37 s → **≤ 0.15 s**; engine/src count 0.40 s → **≤ 0.20 s**; engine/src content 2.25 s → **≤ 0.90 s**.
- **Guardrails (untunable):** parity vs ast-grep exact on the 4 repro queries (`JSON.parse` / await-in-try / push / `$FN` Rust); sanitizer red tests stay green (seeded secrets masked identically); tools-core 1,577 + engine structural 81 tests green.
- **Decision rule:** ACCEPT a change only if a primary improves ≥ 20 % with all guardrails green; REVERT otherwise. Freeze this harness during the loop.

## 4. Known small bugs to fold in
- `output:"countMatches"` on a near-empty result printed `stats: null` on a 1-file dir (observed during research) — check the count-mode stats path for tiny results.
- Profile attribution note: sampled Rust time lands on the JS napi frame (`nativeSanitizeContent`) — fine for bucketing, don't over-read function granularity.

## 5. Sensor (run before/after every change)

```bash
# floor
time node packages/octocode/out/octocode.js tools localSearch --queries \
  '{"operation":"structural","path":"/tmp/tinydir","pattern":"$FN($$$ARGS)"}' --compact >/dev/null
# count + content on the Rust tree (parity: expect ast-grep-identical totals)
time node packages/octocode/out/octocode.js tools localSearch --queries \
  '{"operation":"structural","path":"/ABS/REPO/packages/octocode-engine/src","pattern":"$FN($$$ARGS)","include":["**/*.rs"],"maxFiles":5000,"pageSize":1000}' --compact >/dev/null
ast-grep run -p '$FN($$$ARGS)' --lang rust packages/octocode-engine/src --json=compact | jq length
# sanitizer tax isolated
node -e 'const {sanitizeContent}=require("./packages/octocode-engine/index.js");const t=performance.now();sanitizeContent("clean","/t.txt");console.log((performance.now()-t).toFixed(1),"ms first-call")'
```
