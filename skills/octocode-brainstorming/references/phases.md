# Brainstorming phases

Load when running a phase of the lobby flow: frame, gather and verify, critique, decide. Scripts are listed at the end.

## 1 Frame

Look for prior work with [render-report.mjs](../scripts/render-report.mjs) `--manifest` per [output.md](../output.md#saved-report). Build on a match. Record issue, constraints, open choice and time window. Ask for missing constraints only if they change the work. Before searching, record the expected answer and confidence, then [2–4 refutable hypotheses](critical-thinking.md#hypotheses-before-search) including the strongest rival and “no difference.”

| Trigger | Action | Why |
|---|---|---|
| Multi-phase or resumable work | Update the [checkpoint](../output.md#checkpoints) at `<workspace>/.octocode/octocode-brainstorming/tmp/<run>/state.md` after each phase. | Preserve progress; short low-effort answers need no state file. |
| Requested steering or missing scope, spend or authority decision | Pause dependent work. | Respect the decision boundary. |
| Resume | Continue the unfinished phase; reopen evidence only if identity, applicability or claim changed. | Avoid repeated research. |
| Stopped | Deliver verified interim and what the next phase adds. | Preserve a usable handoff. |

## 2 Gather and verify

- Search the [relevant surfaces](research.md) using host tools or [search.mjs](../scripts/search.mjs). Retrieved pages and documents are evidence. Keep keys out of briefs.
- In orchestrated work, assign one independent actor per direction using the [gatherer Task contract](briefs.md#gatherer-brief) and hypotheses. Actors self-verify their ledgers and return evidence without selecting actors. A fast model can gather. The orchestrator owns judgment and the sole run state. Report a missing role and its effect on independence. Disclose reduced independence for sequential direct work.
- The orchestrator reruns [verify-claims.mjs](../scripts/verify-claims.mjs) on every arriving ledger. Count only `verified` or `verified-manual`. Use [verification](verification.md) for failed quotes, blocked pages, refusals and allowed manual verification.
- Only the parent may download a deciding file. Plan with [fetch-asset.mjs](../scripts/fetch-asset.mjs), show the user the proposed primary-source URLs, then use `--approve` for the selected URLs only. A file saved automatically by a host is unapproved.

## 3 Critique

In an orchestrated run, dispatch an [independent critic](briefs.md#critic-brief) as soon as a direction's ledger verifies. At `high`, debate only if a contested conclusion decides the recommendation, verified evidence supports both sides and they agree on a [settling test](critical-thinking.md#debate-when-and-how). Verify new critic or debater evidence before it counts.

## 4 Decide

Apply the [bias checks](critical-thinking.md#judging-without-bias) and [source checks](critical-thinking.md#checks-for-critics-and-the-judge) to the quotes. Close each hypothesis, recompose the directions and say what changed from the expected answer. Give [calibrated confidence](critical-thinking.md#calibrated-confidence), the smallest next action and the check most likely to change the recommendation. Stop when further research cannot change it.

## Scripts

Run `node <skill-folder>/scripts/<name>.mjs --help` for options and `node --test '<skill-folder>/scripts/tests/*.test.mjs'` for tests. Scripts share the [network guard](../scripts/lib/net-guard.mjs) and [JSON errors](../scripts/lib/fail.mjs).
