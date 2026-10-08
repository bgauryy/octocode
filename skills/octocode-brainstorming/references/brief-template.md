# Idea brief template

Load when the user has agreed to save the record. Why: the file keeps the final findings, every agent, and the debate, which the chat brief does not replace. Ask first. Save under `.octocode/octocode-brainstorming/<date>-<slug>.md` only after a yes. Omit an empty section. This is a check, not a spec. Include `## Resources` when evidence was cited.

```markdown
# Check: {one-sentence issue}
| Field | Value |
|---|---|
| Mode | Validate / Generate / Map |
| Created | {YYYY-MM-DD} |
| Mark | strong / moderate / weak |
| Decision | Build RFC / Prototype First / Narrow / Park / Do Not Build |
| Research limits | {directions not run, and why} |

## Frame
- Issue: {one sentence}
- Context: {what is already true, constraints, who is affected}
- Decision: {what is still open}
- Flip: {the result that would change the decision}

## Directions
- {direction} — {question asked} — {kept or dropped, and why}

## Checks
- Context: {kept or dropped, and the constraint}
- Evidence: {kept or dropped, and the sentence on the page}
- Objection: {kept or dropped, and the contradicting direction}
- Concession: {what changed, or the one drop}

## Final findings
{What survived, the mark, the verdict, and what is still unknown.}

## Agents
One block for every agent. A sequential pass counts. Leave none out.
- **{who}** — direction {question}. Status {complete|partial|blocked}. {claim}. Assumes {context}. `{strong|moderate|weak}` {URL or path:line}. "{supporting sentence}". Falsifier: {what would drop it}.

## Debate
- {side}: {claim, source, and whether it survived}
- Concession: {what this side gave up}
- Parent: {what stayed, and the one drop}

## Next step
{One action: commit, run the smallest test, split, park, or stop.}

## RFC handoff
{Build RFC only: problem, frame, evidence, alternatives, constraints, bounded first step, open questions, success signal.}

## Resources
- {path:line or URL} — {claim supported, author/org/date where unstable, mark}
```
