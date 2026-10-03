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

## Verdict / Risks / Angles / Next Step
<strongest synthesis, unknowns, viable wedge; one next action, no implementation>

## RFC Handoff
<only if ready/requested: problem, framing, value, evidence, alternatives, constraints, first slice, open questions, success signal>

## Sources
- <URL or path:line> — <claim it supports, author/org/date where unstable>
```

When evidence was cited, close with `Sources`: one line per URL/path used above and no new sources. Omit the section for a pure reasoning/framing turn. <!-- style-lint: ignore-line passive-voice -->

## Confidence markers

Every prior-art entry carries a marker. Cite fetched pages, exact code, package metadata, PRs, commits, or tests; snippets are leads.

| Marker | Minimum evidence |
|---|---|
| strong | independent validated sources, or direct code/data plus strong activity/usage |
| moderate | one validated source plus corroborating evidence |
| weak | popularity/marketing/forum only, stale source, or no independent validation |

Marketing stays weak. Present material contradictions. Treat zero prior art as a risk, not a moat.
Next: Build RFC → `octocode-rfc-generator`; save an approved brief with `references/brief-template.md`; Prototype First → test one unknown; Narrow → tighter user/problem; Park → weak evidence/timing; Do Not Build → prior art or risks dominate.
