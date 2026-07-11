# Idea Brief Template
Load when the user approves a saved brief; save under `.octocode/brainstorming/<date>-<slug>.md` or `<doc_placement>`. Exploratory research, not a spec — keep evidence-backed sections; `## Resources` is **mandatory** when external evidence was used, closing with every cited URL/path.
```markdown
# Idea Brief: {one-line restatement}
| Field | Value |
|---|---|
| Status | Exploratory / Validated / Contested / Parked / Too-broad |
| Mode | Generate / Validate / Map |
| Created | {YYYY-MM-DD} |
| Verdict | Crowded / Underserved / Contested / Worth-prototyping |
| Decision | Build RFC / Prototype First / Narrow / Park / Do Not Build |
| Research limits | {skipped/degraded surfaces and why} |

## TL;DR
{Researched framing, verdict, limits, and one next step in 2-3 sentences.}

## Framings Considered
- {angle} — researched / set aside: {reason}

## Already in the Workspace
{Repo-relevant only: existing behavior with file:line; build on vs replace.}

## Landscape — Prior Art
- **{name}** — {surface, behavior, activity/health}. `{strong|moderate|weak}` {URL}

## Perspective Review
- **Critical Architect:** {surviving claim -> evidence -> decision impact -> confidence}
- **Visionary Entrepreneur:** {surviving claim -> evidence -> decision impact -> confidence}
- **Product:** {surviving claim -> evidence -> decision impact -> confidence}
- **Conceded/contested:** {what changed and why}

## Verdict
{Strongest defensible synthesis, disagreement, and key unknowns.}

## Opportunities / Risks / Angles
- {item -> source or weak marker -> implication}

## Recommended Next Step
{One action: prototype, narrow, build RFC, park, or do not build.}

## RFC Handoff
{Build RFC/requested only: problem; framing; value thesis; surviving evidence; alternatives; constraints/risks; bounded first slice; open questions; success signal.}

## Open Questions
- [ ] {cheapest check that could flip the verdict}

## Resources
- {path:line or URL} — {claim supported, author/org/date where unstable, confidence}
```
