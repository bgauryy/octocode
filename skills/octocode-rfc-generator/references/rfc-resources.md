# RFC Resources

Load when producing `RESOURCES.md` or preserving local refs, prior art, papers, packages, research artifacts, and reproducible queries. Why: keep provenance reviewable without replacing inline proof.

```markdown
# Resources: {Title}

## Primary Sources
| Resource | Link or path | Why it matters |
|---|---|---|

## Local Code References
| Area | File and lines | Decision relevance |
|---|---|---|

## Prior Art and Related Systems
| Resource | Link | Lesson for this RFC |
|---|---|---|

## Internal Research Artifacts
| Artifact | Path | What it supports |
|---|---|---|

## Open Research Leads
- {lead} — why it matters and what would make it decision-grade

## Reproducible Search Prompts
- `{query}` — surface and purpose
```

Quality gate:
- Every source says why it matters.
- Local entries use exact `path:line` anchors when possible.
- External entries prefer primary sources.
- Prior art records the lesson, not only the name.
- Leads remain labeled as leads.
- Decisive claims still cite evidence in `RFC.md`, `PREREQUISITES.md`, `IMPLEMENTATION.md`, or `KPI.md`.
- Rows stay dense — no filler entries, no source restated twice.
