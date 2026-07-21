# Loop report — audit FIX + validation update

**Date:** 2026-07-20  
**Subject:** `skills/octocode-orchestrator-local-worker`

## Failures fixed (earlier)

| Issue | Fix |
|---|---|
| HTML entities in SKILL/usage/model-selection | Replaced with real `<` |
| Stale **SELECT MODEL** in decision/invoke/local-models refs | Renamed to **ROUTE** |
| Soft evals masking garbage (`live-classify-tiny` 0.5b, soft article-qwen) | Removed soft cases; classify uses strict 7b only |
| Ambiguous classify schema (`label: bug\|chore\|…`) | Schema requires one exact label |
| Eval accepted legacy 9-step wording | `workflow_select_model` requires GATE→ROUTE→… only |
| `references.md` missing usage-matrix / article dogfood | Provenance rows added |

## Validation update (same day)

| Issue | Fix |
|---|---|
| `kodama-summariser` cited as Local Ollama | Corrected: map-reduce prior art; runtime is **Groq** |
| Missing cascade paper grounding | Added FrugalGPT + cascade-routing + multi-LLM survey to `references.md` |
| Adjacent skills unclear (setup / triage / hermes / subagent) | Added **Not this skill** table + skills.sh re-check notes |
| Soft hard-rules language | MUST/NEVER/FORBIDDEN on verify, invent-model, tool-loop, browse |
| VERIFY as quality gate not explicit | Named quality-estimator + cascade path in SKILL + verify-gate + usage-matrix |

## Verdict

**ACCEPT** (static+script `--skip-live`) — hardFails=0 after restoring eval phrase `Worker never browses the web`.
Live suite last green: 57/57 (pre-edit); re-run live if inventory changes.
