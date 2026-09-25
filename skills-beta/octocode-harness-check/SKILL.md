---
name: octocode-harness-check
description: "Use when auditing how Octocode tools behave for agents in practice: mine real Claude Code sessions with orangu, probe every tool's input/output/context cost on CLI and MCP, trace defects in code with local Octocode tools, and produce a verified fix plan. Not for a single known bug (use octocode-research) or for proving a change improved behavior (use octocode-eval-benchmark)."
---

# Octocode harness check

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Real sessions show where the tools hurt. A live probe shows whether the hurt still exists. Code evidence shows why. The plan turns that into checked changes.

Runs go to `<output>/harness-check/<run-id>/`. Session evidence comes from `npx -y orangu@0.7.2` (pinned).

Flow: `OBSERVE → MINE → PROBE → TRIAGE → TRACE → PLAN → FIX → VERIFY`.

## Steps

1. **OBSERVE** (orangu, read-only, local): from the repo root run
   `npx -y orangu@0.7.2 harness --cwd . --json --quiet > orangu-harness.json` and
   `npx -y orangu@0.7.2 repo . --json --quiet --limit 60 > orangu-repo.json`.
   Keep `byTool` rows for `mcp__octocode*`, `crossFindings`, and `crosswalk.mcpServers`. Field meanings and limits → `references/orangu.md`.
2. **MINE**: orangu redacts error text and does not attribute output size per tool. Run
   `node scripts/mine-transcripts.mjs --cwd . --table` (add `--since <date>` for the current build window) to get per-tool error rates, output-size percentiles, and clustered error signatures with first/last dates.
3. **PROBE**: run `node scripts/probe-surfaces.mjs --out <run>/probe` with `GITHUB_TOKEN`, `OCTOCODE_BETA=true`, and `typescript-language-server` on `PATH`. It runs every tool on native CLI, node CLI, and MCP stdio with a deterministic content check. It records result class, output chars, latency, `next.*` byte share, and MCP catalog weight. If MCP refuses to start with a contract fingerprint mismatch, record that as a finding. Rebuild native, or set `OCTOCODE_ALLOW_CONTRACT_DRIFT=1` only to keep probing, and say so in the report.
4. **TRIAGE** each historical signal into one class before calling it a bug: host/harness (auto-mode outage, user rejection), stale (last seen before the fix, no longer reproduces), agent misuse (wrong input shape, which becomes an error-message quality finding), or live tool defect (reproduces in the probe). Only a live reproduction or a code-proven path counts as a defect.
5. **TRACE** with `octocode-research`, using local tools on the checkout: `localSearch` for the error text or field name → `localFetch` for the deciding lines → `lspSearch` for callers/references → `astSearch` for structural shape. Cite `file:line` for each mechanism. Check for a by-design gate before calling something a bug (for example `ghCloneRepo` is CLI-only).
6. **PLAN**, when triage leaves at least one live finding: write `REVIEW.md` (scorecard, ranked findings, solid areas) and `PLAN.md`. Each row covers finding, owner surface, change, and verification. Freeze the KPI and guardrails first, following the `octocode-eval-benchmark` KPI contract. Guardrails: 0 contract violations, 0 crashes, every `next.*` runnable verbatim. Templates → `references/report-templates.md`.
7. **FIX**: follow the repo's contract flow. Tool schema or description changes go to core, then `yarn contracts:regen`, then the native rebuild (`build:<platform>` + `build:addon`). Output behavior changes go to native. Error rendering goes to the interface (MCP/CLI). Write a failing test first. Split parallel work by disjoint files, and do one regen plus one rebuild at the end.
8. **VERIFY**: rerun the probe into a new directory, then run `node scripts/compare-runs.mjs <baseline>/results.json <candidate>/results.json`. Exit 1 means a regression. A probe gain is a diagnostic KEEP, not an ACCEPT. Behavioral claims ("agents retry less") need an `octocode-eval-benchmark` held-out run.

## Gates

- Measurement never asks a model. orangu counts, `mine-transcripts` clusters, and the probe regex-checks. Judgments come after the numbers.
- Report chars and approximate tokens (chars/4) as estimates. Never convert them to cost.
- orangu output is redacted by default. Add `--include-text` only for local reading, and `--strip-paths` before sharing anything.
- Keep baseline and candidate runs under identical env and binaries. Record binary timestamps and the contract fingerprint (`octocode scheme --compact` → `fingerprint`).
- Never commit. Leave changes in the working tree.
