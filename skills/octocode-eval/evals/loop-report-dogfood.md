## Goal
Agents measure skill changes with `octocode-eval`; claimed post-brainstorm fixes hold under deterministic dogfood.

## KPI
- primary (lagging): `fix_verify_pass_rate` (higher-better) baseline=unknown → treated as 0 until measured result=1.0 target=1.0  [serves goal]
- leading: `eval_eval_self_test`=9/9 · `skill_review_errors`=0 · `fix_checklist`=19/19
- guardrails: no narrative-only accept; harness not edited mid-run; held-out cases still in suite (`tdd-red-green` + prior 8)

## Loop level
experiment (verify-only dogfood — no subject mutation this run)

## Budget / trials
fixed: `eval-eval --self-test` + `loop-report --self-test` + `skill-review` + 19-file checklist · 1 trial

## Subject changed
none (verification of prior ACCEPT patches)

## Harness unchanged? (yes/no)
yes

## Checks run
- `node scripts/eval-eval.mjs --self-test` → pass (9/9), all scores=1.0 including `tdd-red-green`
- `node scripts/loop-report.mjs --self-test` → pass
- `node skills/octocode-skills/scripts/skill-review.mjs skills/octocode-eval` → 0 ERROR, 0 WARN
- fix checklist 19/19: TDD lobby/loop/techniques, case id, harness catalog, description triggers, AGENTS dogfood, research prefers eval (not stale), local-worker routes to eval, monorepo symlinks for eval+research
- held-out: suite cases not used to invent this verify-only run; all 9 strong samples still pass

## Transcript note
Dogfood Mode:Run. Deterministic graders only — no LLM judge.

## Verdict
ACCEPT

## Next
Optional meta: add `evals/kpi-contract.json` for this skill’s own improve loop (parity with local-worker).
