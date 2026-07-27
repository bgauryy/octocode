# Octocode Benchmark

Two lanes, both docs-driven and run by an agent following the checks. No rigid
generated trees — just questions, checks, and frozen ground truth.

```
packages/octocode-benchmark/
  BENCHMARK.md            ← this file
  package.json
  benchmark/
    per-tool/             ← one file per Octocode tool: full schema + checks + workflows
      README.md           ← how to run + shared 2/1/0 scoring
      <toolName>.md       ← one doc per active/default tool plus gated legacy docs
    compare/              ← Octocode vs others, 10 questions each
      README.md           ← shared A/B method + metrics
      octocode-vs-gh/           ← Arm A: gh CLI only
      octocode-vs-gh-rtk/       ← Arm A: rtk + gh CLI (ground truth verified)
      octocode-vs-ast-grep/     ← Arm A: ast-grep CLI
```

## Lane 1 — Per-Tool ("does our own surface work")

One file per active/default tool proves it works across **its whole schema** and in **real
workflows**. This is the internal lane — run it before trusting any comparison.

Start here: [`benchmark/per-tool/README.md`](benchmark/per-tool/README.md).

```bash
CLI="node packages/octocode/out/octocode.js"
$CLI tools <name> --scheme --compact          # authoritative schema
$CLI tools <name> --queries '<json>' --compact # run a check
```

Every check scores **2 / 1 / 0** (or honest `N/A`). A tool passes when every
check scores 2 and every workflow reaches its stated proof.

## Lane 2 — Compare ("is Octocode better than the alternatives")

Three head-to-head suites, **10 questions each**, same LLM + same tasks + same
budget — only the tool provider changes.

| Suite | Arm A | Arm B |
|---|---|---|
| [octocode-vs-gh](benchmark/compare/octocode-vs-gh/) | `gh` CLI | Octocode CLI |
| [octocode-vs-gh-rtk](benchmark/compare/octocode-vs-gh-rtk/) | `rtk` + `gh` CLI | Octocode CLI |
| [octocode-vs-ast-grep](benchmark/compare/octocode-vs-ast-grep/) | `ast-grep` CLI | Octocode CLI |

Each suite: `questions.md` (solver-facing, frozen), `ground-truth.json`
(judge-only), `README.md` (map + notes). Method + metrics (tokens, turns,
wall-clock, correctness, false-confidence) live in
[`benchmark/compare/README.md`](benchmark/compare/README.md).

**Decision rule:** Octocode wins a suite only if mean correctness improves
**and** token cost doesn't regress past a pre-registered ceiling.

## Prerequisites

- Built CLI at `packages/octocode/out/octocode.js` (rebuild:
  `cd packages/octocode-engine && yarn build`, then build octocode).
- `OCTOCODE_TOKEN` + network for GitHub/npm checks and the compare suites.
- Baselines for compare: `gh` CLI, `rtk`, `ast-grep` installed for their arms.

## Rules

- `--scheme` is the source of truth. If a check's params drift from `--scheme`,
  fix the check.
- Ground truth is verified by a method **outside both arms** and is
  **time-sensitive** — re-verify PR/issue state, line numbers, and counts before
  a scored run; record the date.
- Report dropped/timed-out questions explicitly. Snippets are discovery, not
  proof.
