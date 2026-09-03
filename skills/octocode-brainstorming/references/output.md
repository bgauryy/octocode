# Brainstorm Output

Load when presenting the chat brief, assigning confidence, or preparing an RFC handoff. Present in chat first; save only after approval using `brief-template.md` and `<doc_placement>`.

```markdown
# Idea: <restatement> · Verdict: <crowded|underserved|contested|worth-prototyping> · Decision: <Build RFC|Prototype First|Narrow|Park|Do Not Build>

## TL;DR
<researched framing, verdict, limits, and one next step>

## Direction Check
<user choice, focused question, or explicit assumption>

## Framings Considered
- <angle — researched or set aside>

## Already in the Workspace
<repo-relevant only: file:line, current behavior, build-on vs replace>

## Evidence by Surface
- **<source>** — <claim and signal>. `<strong|moderate|weak>` <URL or file:line>

## Perspective Review / Decision Delta
- Architect / Entrepreneur / Product: <what survived>
- Conceded or contested: <what changed and why>

## Verdict / Risks / Angles
<strongest synthesis, unknowns, and viable wedge>

## Recommended Next Step
<one action; no implementation>

## RFC Handoff
<only if ready/requested: problem, framing, value, evidence, alternatives, constraints, first slice, open questions, success signal>

## Sources
- <URL or path:line> — <claim it supports, author/org/date where unstable>
```

When evidence was cited, close with `Sources`: one line per URL/path used above and no new sources. Omit the section for a pure reasoning/framing turn. <!-- style-lint: ignore-line passive-voice -->

Confidence markers for `Evidence by Surface` entries, and Decision-label routing: `references/confidence.md`.
