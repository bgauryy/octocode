# RATE
Load after UNDERSTAND and before FIX. Why: evidence and severity keep the repair proportional.
## Issue Scan
| Severity | Categories |
|---|---|
| Critical | weak modal inside a truly critical rule; safety/permission conflict |
| High | missing enforcement; ambiguous action/referent; intent-changing contradiction |
| Medium | missing output/gate; duplication; low density; imprecise term; buried rule; unmarked example/data; irrelevant metadata |
| Low | indirect/wordy sentence; repeated example; cosmetic residue |

Keep optional modals optional. Preserve required frontmatter and exact commands.

## Score

Score 1–5, then average: A 4.5–5 · B 3.5–4.4 · C 2.5–3.4 · D <2.5.
| Dimension | 5 means |
|---|---|
| Clarity | concrete verbs, stable terms, explicit referents |
| Enforcement | proportionate boundaries and phase gates |
| Structure | visible order; separated examples/data where needed |
| Density | every section constrains behavior; one owner per rule |
| Output | concrete shape for every required deliverable |
| Integrity | preserved intent, metadata, branches, and commands |
```markdown
## Issues Found
| Part | Issue | Severity | Fix |
|---|---|---|---|
| <part> | <problem> | Critical/High/Medium/Low | <bounded change> |
## Score
| Dimension | Before (1-5) | Evidence |
|---|---:|---|
| <dimension> | <n> | <why> |
Overall: <avg> → <grade>
```
Rate every logical part, cite evidence, and avoid inflating severity. If the scan is unexpectedly clean, recheck modals, referents, conflicts, branches, and outputs once.

## Sources
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — evaluate real tasks, inspect transcripts, and measure tool behavior.

Next: with issues and a before score recorded load `references/fix.md`; when a severity rests on assumed behavior rather than evidence load `references/evaluation-data.md` first; when the issue is a precedence conflict load `references/patterns.md`.
