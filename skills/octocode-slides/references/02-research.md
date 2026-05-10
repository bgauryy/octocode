# Phase 2 — Research

**Role:** Research agent. You collect enough evidence to build a credible deck — facts, code, context, comparisons, quotes, and data. Research is a sufficiency exercise, not an exhaustiveness exercise. Keep planning and design decisions for later phases.

**Input:** `.content/brief.md`
**Output:** `.content/research.md`

---

## Skip gate — read first

Read `.content/brief.md` now. If **all** of the following are true, skip to Step 5 immediately:
- The brief has no gaps listed
- Source files cover all key points (facts, code, data)
- User said "skip research", "quick deck", or "no research needed"

Otherwise continue with Step 1. Keep the research agenda tied to known gaps and the audience's decision need.

---

## Step 1 · Read the brief

Extract:
- Topic and audience
- **Depth level** (Executive / Management / Technical / Mixed / Async) — this determines what kind of evidence to prioritize:

| Depth level | Prioritize finding |
|-------------|-------------------|
| Executive | Business outcomes, ROI data, risk statements, market stats |
| Management | Trade-off comparisons, feasibility evidence, progress indicators |
| Technical | Working code, benchmarks, architecture diagrams, failure modes |
| Mixed | One strong narrative hook + technical proof in separate sections |
| Async | Self-explanatory charts, step-by-step flows, full context per slide |

- Source files already read in Phase 1 (skip re-reading those)
- Known gaps — these drive the research agenda

---

## Step 2 · Deep-read source files (if not done in Phase 1)

If source files were listed in brief.md but not yet read, read them now with Octocode/local tools when available:

```
Read each provided source file
```

For code repositories or large folders:
```
View structure at useful depth
Search code for key concepts from the brief
Use LSP/navigation to trace key functions or types when needed
```

Fallback if local/LSP tools are unavailable: use `rg --files`, `rg`, `sed`, `nl`, and targeted file reads.

Extract: key facts, code patterns, architecture decisions, quotes worth featuring.

---

## Step 3 · Web research

**When:** Topic needs context, the user wants data / stats / case studies, or brief has knowledge gaps.

**Skip when:** User provided complete source materials that cover all key points, or the topic is internal/proprietary and web results would add nothing, or user said "skip research" / "quick deck".

Pick the queries that match the topic — run in parallel with whatever web/search tools are available in the current agent environment:

| Query type | How |
|------------|-----|
| Best practices | Fetch authoritative docs or known articles directly; use web search only when discovery is needed |
| Statistics / data | Search `"<topic> statistics latest"` or `"<topic> data {{current year}}"`, then read the most credible primary/official sources |
| Case studies | Search `"<topic> real-world examples"` and read the most relevant original source |
| Comparisons | Search `"<topic A> vs <topic B>"`, then verify specifics from primary docs where possible |
| Definitions / context | Fetch official docs, standards, or spec pages directly |

**Tool order:** Use known primary URLs first (fastest and most reliable). Use web search when you need to discover URLs. Follow leads — a good article links to better sources.

Fallback if web tools are unavailable: use official documentation already present in the workspace, GitHub pages available through local/Octocode tools, and local shell reads. Record the actual URL or file path used either way.

Record: what was found, where it came from (URL), and what slide it supports.

If a needed claim cannot be validated with user sources, Octocode/local tools, or web research, ask the user for the missing source or mark it as an assumption. Keep unvalidated assumptions out of slide headlines and chart values.

---

## Step 4 · Octocode / GitHub research

**When:** Deck needs real code samples, API references, library docs, or technical architecture examples.

```
Octocode/GitHub repository search for "<topic> <language>"      ← find canonical repos
Octocode/GitHub code search for "<key pattern>" in credible repos ← find real implementations
Read file content from the repo                                  ← README, spec, API doc, or focused source file
```

Prefer code that is:
- From a repo with credibility (stars, maintenance, known org)
- Short enough to fit on one code slide (≤ 20 lines)
- Directly illustrating a key point in the deck

---

## Step 5 · Write research.md

Create `.content/research.md` inside `.octocode/slides/{{slideName}}/`. Keep it concise: facts and evidence that may appear in the deck, not a general literature review.

```markdown
# Research: {{Title}}

## From user source files

### {{filename or path}}
{{Verbatim or paraphrased content. Key facts, quotes, code snippets.}}

---

## From web

### {{URL — page title}}
{{Extracted facts, data, quotes. Cite URL.}}

---

## From GitHub

### {{owner/repo — description}}
{{Key code samples, architectural insight. Include GitHub link.}}

---

## Facts and data to feature

| Claim | Source |
|-------|--------|
| {{stat or insight}} | {{URL or path}} |

## Code samples to feature

\```{{language}}
// Source: {{URL or filename:line}}
{{snippet — max 20 lines}}
\```

## Gaps remaining

{{Anything that couldn't be found. Flag it so the outline agent knows.}}
{{If none: "None."}}
```

---

## Gate 2 — No user approval needed

When research.md is complete, confirm to the user and move on immediately:

```
Research complete →

Sources: {{n user files}} · {{n web pages}} · {{n GitHub repos}}
Key findings: {{2–3 headline facts}}
Code samples: {{n}} ({{languages}})
Gaps: {{list or "none"}}

→ Phase 3: building the outline
```
