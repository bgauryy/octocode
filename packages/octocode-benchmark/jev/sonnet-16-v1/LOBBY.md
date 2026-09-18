---
name: sonnet-16-v1
description: One-run A/B pilot measuring whether the Jev reasoning loop improves Sonnet-driven octocode research on 16 grounded cases (10 canonical GitHub questions + 6 real large-repo issues). Baseline uses octocode only; treatment adds exactly one live Jev call at the decision fork. Report-only.
---
# Jev A/B benchmark — sonnet-16-v1

suite: `sonnet-16-v1` | host: `claude-sonnet-5` (both arms identical) | jev: `jev-1.13.0` (pinned)
cases: `cases.json` (Q1–Q10 canonical + ISS1–ISS6 real issues) | harness: `harness.mjs` | output: `RESULTS.md`

**One question, isolated:** does adding the Jev decision loop change octocode research conclusions and improve them? The only variable between arms is Jev. Everything else — model, task text, tool cap, judge — is held equal. This is the matched host-with-vs-without-Jev baseline the standalone skill's own benchmark cannot supply alone.

## Cases (16)
- **10 questions** (`Q1`–`Q10`): canonical GitHub source-tracing questions, copied unchanged from `../questions.json`. Verifiable at ref; possible prior model knowledge is a known limitation.
- **6 issues** (`ISS1`–`ISS6`): real, octocode-verified on 2026-09-18 — `facebook/react` #37637 & #37619, `langchain-ai/langchain` #40592 & #40590, `vercel/next.js` #49169 & #45508. Issue bodies often carry a proposed cause, so these test *verification and fix quality*, not blind discovery.

## Arms (per case)
| Arm | Octocode | Jev | Returns |
|---|---|---|---|
| `baseline` | ≤ 7 calls, cite source at ref | 0 calls | answer + sources@ref + call count |
| `jev` | ≤ 7 calls, same cap | exactly 1 live `run-loop.mjs` call at the key fork | same + jev route/choice/probability/gate + jev tokens |

Establish behavior from source or diff at ref — PR/issue prose is context, not proof. A treatment case that makes 0 Jev calls is recorded but yields no treatment evidence.

## Judge
One blinded judge per case sees the two answers as "Response 1/2" in randomized order (index parity), independently reopens cited sources at ref (≤ 4 octocode calls), and scores each 0–5 on **correctness**, **research quality** (claims backed by exact source@ref), and **efficiency** (grounding per octocode call). De-blinded only in aggregation. Winner per case + reasoning.

## Measurement boundary (honest)
Per-agent host LLM tokens are not exposed to the harness → efficiency is judged from octocode call counts + citation quality. Jev's own API cost is measured exactly from run-loop metrics. Absolute characterization, not a cost-parity proof.

## Decision rule
Report-only pilot, one run per case. A positive paired delta suggests a larger sealed replication; a flat or negative delta does not justify default Jev routing. Do not edit cases, cap, or rubric mid-run. Never overwrite a sealed suite — cut a new version.

## Run
```sh
# harness.mjs is a Workflow script (48 Sonnet agents: 16 x 2 arms + 16 judges)
# launched via the Workflow tool with scriptPath: sonnet-16-v1/harness.mjs
```
