# Output

<!-- report-contract {"statuses":["Recommended","Open","Rejected"],"required":["Directions","Recommendation","Dissent","Limits","Sources"],"anyOf":[]} -->

Deliver task content only; no probe output or process metadata.

## Response

BLUF: recommendation and confidence in one line; then the issue, constraints, and directions (table); end with the strongest objection and the next check.

| Direction | Conclusion and why | Sources and relevant dates | Quality, objections, and uncertainty |
|---|---|---|---|

Cells: one or two short sentences. Longer reasoning goes in a `### <direction>` paragraph below: why it was retained, revised, or rejected.

- Every factual sentence says no more than its verified quote; mark inference; cut the rest.
- Dates and venues only from verified pages or arXiv ids. As-of date for changing evidence: the source's date or `date +%F`, never a remembered date.
- Confidence `high`, `medium`, or `low` with its [rubric](references/critical-thinking.md#calibrated-confidence) reason and the check that would move it; never a percentage.
- One fenced block per artifact.
- State material limits (verified and dropped counts, reduced independence), no worker roster or tool inventory.
- Each framed hypothesis: survived, refuted (by which source), or untested.
- Before saving, check: every number sourced or marked estimate, no date past the as-of date, no contradictions, no benefit the directions did not show.

End every response, even a short chat answer, with a `Sources` list, one line per verified source: a Markdown link labeled with publisher or arXiv id and short title, targeting the exact verified URL (never a bare URL in prose), then what it supports, source type, and date. Mark secondary pages; give the dropped and unverified count in one line; list every cited source, and say how many more a saved brief holds. With no verified source, say so.

## Low effort

A few sentences or bullets: answer, verified evidence, strongest caveat, confidence with reason, what a deeper run would add. No table. Sources list. Several questions in one reply: effort, verified and dropped counts once for the reply; a caveat or next check only where one exists.

## Checkpoints

Each phase, overwrite `<run>/state.md`, a Checkpoint–resume record (goal as the question, done as the phase, next, files as ledgers, plus the expected answer, hypotheses, and interim answer), with every field current:

```markdown
# <question>
effort: low | medium | high
phase: 1 frame | 2 gather | 3 critique | 4 decide (last finished)
next: <the next phase and what it will add>
expected answer: <answer and confidence written before research>
hypotheses: <H1 ... with status survived | refuted | untested | open>
ledgers: <path — verified/dropped counts, per direction>
interim answer: <one to three sentences from verified claims only>
```

Show `Interim (phase <n> of 4)`: interim answer with confidence, what is open, what the next phase adds, Sources. When stopped, end with "Say continue to run <next phase>."

## Think together

Respond with the framing before or instead of a full brief: the user's view in one line; sub-questions marked known, assumed, or researched; options labeled by origin (user or agent); hypothesis statuses; the answer with evidence; the 1–3 questions that would most change the decision; what changed from the expected answer; Sources.

## Presentation choices

Recommendation → compared directions → strongest dissent → limits and sources. Put extended reasoning below a compact comparison.

| Format | Use when / why |
|---|---|
| Comparison table | Compare options against the same explicit criteria and evidence. |
| `quadrantChart` / `radar-beta` | Show two-axis tradeoffs / comparable profiles only when placements, scales and estimates are explained. |
| `mindmap` / `flowchart` | Explore concepts / explain the decision path; these do not prove the recommendation. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

| Result | Where it goes |
|---|---|
| Comparison of two or more directions, or any result the user or project instructions (such as `AGENTS.md`) ask to save | Save at the user-named path if given, else the default path below |
| Short single-direction answer | Chat |
| A harness or subagent scratch folder (`.octocode/tmp/`) | Never the destination: save at the default path and link from scratch notes |

Default path: `<workspace>/.octocode/octocode-brainstorming/<YYYY-MM-DD>-<slug>.md`, where `<workspace>` is the repository root, else the current directory, never inside an existing `.octocode/`.

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Recommended`, `Open`, `Rejected`.

Follow the header with the `#` title and a lead paragraph that states the answer and why. Always include `## Directions` (the comparison table under Response, with incomplete or blocked directions), `## Recommendation` (the best-supported choice and why it beat the others), `## Dissent` (the strongest objection and what would make it win), `## Limits` (evidence limits and as-of dates), and `## Sources` (the list under Response, then a link to the run's ledgers).

Add these sections only when they have content, in this order around the required ones:

- `## Context`, before Directions: issue, constraints, known facts, and evidence that could change the choice.
- `## Hypotheses`, before Directions: a table with columns Hypothesis, Refuted if, Result (Survived, Refuted, or Untested), and Deciding sources.
- `## Comparison`, after Directions: which directions survived, changed, or were rejected, and why; evidence vs inference; reconciled contradictions; material dissent.
- `## Next action`, after Limits: the smallest useful test, step, narrower question, or reopening signal. For an RFC handoff, list the decision, constraints, alternatives, evidence, acceptance criteria, and open questions.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
