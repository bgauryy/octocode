# Loop report — serving knobs + live suite

**Date:** 2026-07-21  
**Subject:** `skills/octocode-orchestrator-local-worker`

## Goal
Agents offload low-risk work via a 5-step portable flow, with verifiable worker output and sound Ollama serving knobs (keepalive, ctx, structured temp).

## KPI
- primary (lagging): `suite_pass_rate` (higher-better) baseline=1.0 result=1.0 target=0.85
- leading: script_gate=1.0 · live_worker_hard=1.0 · held_out_hard=1.0
- guardrails: ok (keepalive default, HTTP temp/num_ctx, embed/fuzzy/think/tool/image-gen)

## Loop level
experiment (subject: worker script + docs + cases for serving knobs)

## Budget / trials
static+script+live · ≤25 live calls · skip qwen2.5:32b · ~133s wall

## Subject changed
- `scripts/ollama-worker.sh`: default `--keepalive 5m`; `--temperature` / `--num-ctx` → `/api/generate`
- Docs: SKILL RUN/Recovery, ollama-invoke serving table, cli/usage/packet/family
- Eval cases: keepalive/temp dry-run gates; structured live jobs use `temperature: 0.2`
- Discovery: skill-sync to top vendors + project scopes; AGENTS.md dogfood row

## Harness unchanged? (yes/no)
no — suite evolved between experiments (new serving-knob cases/guards only)

## Checks run
- `node scripts/eval-skill.mjs` → exit 0 · 62/62 · hardFails=0 · held-out 5/5
- report: `.octocode/orchestrator-local-worker/evals/last-report.json` (temp; not under `evals/`)
- `skill-review.mjs` → 0 errors (after README + description tighten)
- skill-sync `--platforms top --project-root … --approve --force` → 7 linked

## Transcript note
Live path exercised vision, translate, article grounding, JSON extract/classify/check with keepalive + low-temp HTTP.

## Verdict
**ACCEPT**

## Next
No further subject change required for this KPI. Optional later: split long refs (review WARNs only).
