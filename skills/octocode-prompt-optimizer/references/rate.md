# RATE Gate

Load when rating a prompt for issues, after UNDERSTAND and before FIX.

**STOP. DO NOT fix anything yet. Rate each logical part for issues first.**

### Pre-Conditions
- [ ] UNDERSTAND gate completed
- [ ] Understanding output produced

### Issue categories (check all)
Severity in brackets. Scan every logical part against each.

- `weak-words` — "consider", "might", "could", "may", "should" inside a critical rule. [Critical]
- `missing-enforcement` — rules with no Forbidden/Allowed boundary. [High]
- `ambiguous-instruction` — "do some", "handle", "process" with no specifics. [High]
- `referential-ambiguity` — "it", "this", "that", "above", "below" with no clear antecedent. [High]
- `missing-output-format` — expected outputs with no template. [Medium]
- `missing-gate` — a phase transition with no checkpoint. [Medium]
- `duplication` — the same rule stated in multiple places (not examples). [Medium]
- `verbose-bloat` — a section over 20 lines that a table would carry; prose with no constraint. [Medium]
- `wordy-indirect` — nominalizations, passive voice, expletive openers ("there is/it is"), double negatives, or compound sentences over about 25 words (see `conciseness-toolkit.md`). [Low]
- `imprecise-wording` — a catch-all verb ("handle", "process"), a concept named by shifting synonyms, or a rare word where a plain one reads faster (see `attention.md`). [Medium]
- `buried-critical-rule` — a MUST/NEVER/FORBIDDEN sitting in the middle of a long section instead of its start or end, where attention drops (see `attention.md`). [Medium]
- `unmarked-example-or-data` — literal exemplars or reference data mixed into instructions with no `<example>`/`<context>` tag, so the agent may execute them as commands (see `attention.md`). [Medium]
- `emoji-as-instruction` — an emoji used as a command instead of a strong word. [Medium]
- `redundancy` — a repeated example or needless variation. [Low]
- `low-density` — an explanation that does not constrain behavior. [Low]
- `meta-noise` — the text talks about the prompt instead of instructing ("this prompt will…", author/TODO notes), carries metadata or frontmatter that is not part of the contract, or restates content already stated elsewhere. [Medium]

### Best-practices score (produce after the issues scan)
Score each dimension 1–5 (1 = fails, 5 = exemplary), then average to a grade. Dimensions reflect current prompt-engineering guidance and recent long-context/attention studies.

| Dimension | 5 = exemplary | Grounding |
|-----------|---------------|-----------|
| Clarity & word choice | concrete verbs, one term per concept, no ambiguous referents | semantic misreading is the top model-error source |
| Enforcement & gates | critical rules use strong modals; phase transitions checkpointed | reliability from explicit boundaries |
| Structure & attention | front-loaded frame; critical rules bracketed at start/end; examples/data tagged | U-shaped primacy/recency attention bias |
| Density & conciseness | high signal-per-token; no bloat, no meta-noise | prompt-compression / token-efficiency work |
| Output contract | every expected output has a template | consistent, parseable results |
| Integrity | intent preserved; no metadata, scaffolding, or redundant content leaked | prompt does one job, states it once |

Grade the average: **A** 4.5–5 · **B** 3.5–4.4 · **C** 2.5–3.4 · **D** below 2.5. FIX targets the lowest-scoring dimensions first.

### Rating output
```markdown
## Issues Found
| Part | Issue | Severity | Fix Needed |
|------|-------|----------|------------|
| [Part name] | [Description] | Critical/High/Medium/Low | [What to do] |

## Best-Practices Score
| Dimension | Score (1-5) | Note |
|-----------|-------------|------|
| Clarity & word choice | [n] | [why] |
| Enforcement & gates | [n] | [why] |
| Structure & attention | [n] | [why] |
| Density & conciseness | [n] | [why] |
| Output contract | [n] | [why] |
| Integrity | [n] | [why] |
**Overall: [avg] → [A/B/C/D]**
```

### Gate Check
- [ ] All logical parts rated
- [ ] Weak-word scan completed
- [ ] Issues table produced
- [ ] Severity assigned to each issue
- [ ] Best-practices score produced with an overall grade

### Forbidden
- Fixing issues before rating is complete
- Ignoring critical issues
- Skipping the weak-word scan

### Allowed
- Text output (issues table)
- Re-reading parts for rating

### On Failure
- **IF** no issues found → **THEN** double-check with a weak-word scan.
- **IF** the scan is still clean → **THEN** record "No issues found" and proceed.

## Weak-word reference
Replace weak words only where the rule is critical; keep them where guidance is genuinely optional.

- `consider-might-could-may` — critical section → **MUST** / **REQUIRED**; optional guidance → remove, or keep with "optionally".
- `should-prefer` — critical section → **MUST**; soft guidance → keep as-is.
- `do-some-handle-process` — any context → name the exact action ("Run X", "Call Y").
- `as-needed-if-necessary` — any context → rewrite as **IF** [condition] → **THEN** [action].
- `feel-free-you-can` — required action → remove, use **MUST**; optional action → "Optionally, you may…".

Replace any weak word that sits inside a critical (must/never/forbidden) rule.
