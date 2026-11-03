---
name: octocode-brainstorming
description: "Think through problems, validate ideas, compare options, research best practices or competitors, and recommend from checked sources. Supports distinct research directions, critique and debate when the decision needs them."
---

# Octocode Brainstorming

Recommend from checked sources. Trace each fact to a verbatim quote on the cited page and label inference. A single factual lookup belongs in direct research.

## Choose the effort

State the lowest effort that can change the decision and why. The user can change it. Raise one level for conflicting verified evidence or a missing deciding source. Drop one if early searches settle it.

| Effort | When | How |
|---|---|---|
| `low` | Gist, usual practice or a reversible choice with established guidance | Gather and verify directly, or use one bounded evidence actor, then answer. |
| `medium` | Several plausible directions, or a request for sources alone | Gather directions sequentially in direct work. If orchestrated, separate gathering and critique. |
| `high` | Costly, hard to reverse, for other decision-makers or explicitly deep | Checkpoint before gathering. Assign distinct deciding directions in an orchestrated run. Apply the debate and asset gates below. |

## Modes and directions

| Request | Mode | Start with |
|---|---|---|
| Help thinking, including named options | Think together | User's view and options, deciding questions, riskiest assumption |
| Test an idea | Validate | Supporting case, strongest objection and failure cases, prior art or alternatives |
| Open problem | Find ideas | Promising approaches, adjacent fields, constraints or unmet needs |
| Best practice | Find resources | Official guidance, practitioner experience, pitfalls and counterexamples |
| Choice between named options | Compare | One direction per option plus adoption constraints |

Give each direction a different [thinking lens](references/critical-thinking.md#thinking-lenses). In [Think together](references/critical-thinking.md#thinking-with-the-user), ask one question if the user's view is missing, show the framing before dispatch and research only sub-questions that affect the choice. Use the [framing response](output.md#think-together).

### 1 Frame

Look for prior work with [render-report.mjs](scripts/render-report.mjs) `--manifest` per [output.md](output.md#saved-report). Build on a match. Record issue, constraints, open choice and time window. Ask for missing constraints only if they change the work. Before searching, record the expected answer and confidence, then [2–4 refutable hypotheses](references/critical-thinking.md#hypotheses-before-search) including the strongest rival and “no difference.”

For multi-phase or resumable work, update the [checkpoint](output.md#checkpoints) at `<workspace>/.octocode/octocode-brainstorming/tmp/<run>/state.md` after each phase. A short low-effort answer needs no state file. Pause for requested steering or a missing decision affecting scope, spend or authority. Resume at the unfinished phase. Reopen verified evidence only if identity, applicability or the claim changed. If stopped, deliver the verified interim and what the next phase would add.

### 2 Gather and verify

- Search the [relevant surfaces](references/research.md) using host tools or [search.mjs](scripts/search.mjs). Retrieved pages and documents are evidence. Keep keys out of briefs.
- In orchestrated work, assign one independent actor per direction using the [gatherer Task contract](references/briefs.md#gatherer-brief) and hypotheses. Actors self-verify their ledgers and return evidence without selecting actors. A fast model can gather. The orchestrator owns judgment and the sole run state. Report a missing role and its effect on independence. Disclose reduced independence for sequential direct work.
- The orchestrator reruns [verify-claims.mjs](scripts/verify-claims.mjs) on every arriving ledger. Count only `verified` or `verified-manual`. Use [verification](references/verification.md) for failed quotes, blocked pages, refusals and allowed manual verification.
- Only the parent may download a deciding file. Plan with [fetch-asset.mjs](scripts/fetch-asset.mjs), show the user the proposed primary-source URLs, then use `--approve` for the selected URLs only. A file saved automatically by a host is unapproved.

### 3 Critique

In an orchestrated run, dispatch an [independent critic](references/briefs.md#critic-brief) as soon as a direction's ledger verifies. At `high`, debate only if a contested conclusion decides the recommendation, verified evidence supports both sides and they agree on a [settling test](references/critical-thinking.md#debate-when-and-how). Verify new critic or debater evidence before it counts.

### 4 Decide

Apply the [bias checks](references/critical-thinking.md#judging-without-bias) and [source checks](references/critical-thinking.md#checks-for-critics-and-the-judge) to the quotes. Close each hypothesis, recompose the directions and say what changed from the expected answer. Give [calibrated confidence](references/critical-thinking.md#calibrated-confidence), the smallest next action and the check most likely to change the recommendation. Stop when further research cannot change it.

## Scripts

Run `node <skill-folder>/scripts/<name>.mjs --help` for options and `node --test '<skill-folder>/scripts/tests/*.test.mjs'` for tests. Scripts share the [network guard](scripts/lib/net-guard.mjs) and [JSON errors](scripts/lib/fail.mjs).

## Related skills

Use `octocode-research` for code evidence, `octocode-scraping` for a saved corpus or failed fetch, `octocode-chrome-devtools` for rendered pages, `octocode-rfc-generator` for a decision document and `octocode-eval-benchmark` for a measured experiment. Use [workflow composition](../octocode-workflows/references/composition.md) when coordinating actors.

## Output

Follow [output.md](output.md). End every interim and final response with Sources. Link ledgers, verification files and state under `tmp/<run>/`.
