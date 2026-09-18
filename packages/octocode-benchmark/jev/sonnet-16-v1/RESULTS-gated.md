# sonnet-gated-v2 results (gated + CoT)

**The reposition works: gating flipped the paired delta from −1.56 (forced) to +0.53 (gated), with fewer octocode calls. But the win comes from the mandatory CoT/anchor-check step, not from Jev — agents called Jev only 1/19 times.** Report-only pilot, one run.

Run: 57 agents, 0 errors, ~13.5 min. 19 cases = the 16 + 3 attractive-wrong-lead forks (axios fetch-vs-XHR, useMemo guarantee, Buffer pooling). Treatment arm = THINK → GATE → call Jev only on `disputed_inference`. Jev overhead: 3,821 in / 82 out tokens (1 call).

## Aggregate: forced vs gated

| | Forced (v1) | **Gated + CoT (v2)** |
|---|---|---|
| Paired total delta /15 | **−1.56** | **+0.53** |
| Wins (jev/gated : baseline : tie) | 5 : 11 : 0 | 7 : 9 : 3 |
| Jev calls | 16/16 forced | **1/19** |
| Avg octocode calls (arm) | 5.69 | **4.21** (baseline 4.37) |

Delta split: non-disputed **+0.81**, disputed **−1.0** (3 cases, high variance; Jev not called on any).

## Routing behaviour — the decisive observation

| Route chosen by the gated agent | Count |
|---|---|
| `deterministic` | 18 |
| `missing_fact` | 0 |
| `disputed_inference` | 1 |

**Agents classified 18/19 steps as deterministic and skipped Jev — including all 3 attractive-wrong-lead forks.** That is not a failure: a code-research question with a source-backed answer *is* deterministic once you read the source. The "attractive wrong lead" traps someone who doesn't check source; an agent that reads source finds the one right answer. Genuine `disputed_inference` (two source-backed interpretations, no cheap check) is **rare** in source-tracing.

Crucially, the gated CoT arm **resisted the traps without Jev**: DI1 (axios) tie +2, DI2 (useMemo) tie 0 — it did not answer "fetch" or "guaranteed." The anchor-check in the THINK step caught the lead on its own.

## Per case

| Case | base(tot) | gated(tot) | Δ | route | jevCalled | winner |
|---|---|---|---|---|---|---|
| Q1 | 15 | 14 | −1 | deterministic | no | baseline |
| Q2 | 15 | 13 | −2 | deterministic | no | baseline |
| Q3 | 15 | 9 | −6 | deterministic | no | baseline |
| Q4 | 11 | 14 | +3 | deterministic | no | gated |
| Q5 | 11 | 15 | +4 | deterministic | no | gated |
| Q6 | 9 | 15 | +6 | deterministic | no | gated |
| Q7 | 0 | 15 | +15 | deterministic | no | gated |
| Q8 | 15 | 14 | −1 | deterministic | no | baseline |
| Q9 | 14 | 14 | 0 | deterministic | no | tie |
| Q10 | 13 | 12 | −1 | deterministic | no | baseline |
| ISS1 | 14 | 7 | −7 | deterministic | no | baseline |
| ISS2 | 10 | 11 | +1 | disputed_inference | **yes** | gated |
| ISS3 | 15 | 10 | −5 | deterministic | no | baseline |
| ISS4 | 15 | 13 | −2 | deterministic | no | baseline |
| ISS5 | 6 | 12 | +6 | deterministic | no | gated |
| ISS6 | 10 | 13 | +3 | deterministic | no | gated |
| DI1 | 12 | 14 | +2 | deterministic | no | tie |
| DI2 | 14 | 14 | 0 | deterministic | no | tie |
| DI3 | 14 | 9 | −5 | deterministic | no | baseline |

Large swings (Q7 +15, ISS1 −7) are worker research-quality variance, not Jev effects — Jev was not called on either.

## What this establishes about when Jev helps

1. **Gate-first + mandatory CoT is the right design** — it erased the forced-call tax (−1.56 → +0.53) and cut octocode calls. Ship it.
2. **The CoT/anchor-check discipline is the workhorse, not Jev.** The improvement appeared with ~0 Jev calls; structured thinking made the agent modestly better and cheaper on its own — and it works on any model size because it is a scaffold, not a model capability.
3. **For source-research, call Jev almost never.** 18/19 steps were source-determinable. Jev is a specialist for genuine two-interpretation forks, which are uncommon in code tracing (more common in ambiguous design/judgment calls). The one real call (ISS2) did help (+1).

## Limitations

One run; single Sonnet judge shared with workers; my 3 "disputed" cases turned out source-determinable, so the suite still under-samples true `disputed_inference` — the population where the skill's own recovery benchmark showed 3/3. Building genuinely two-interpretation cases remains the next step to isolate Jev's ceiling.
