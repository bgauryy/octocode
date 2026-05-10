# Phase 2 — Research

**Role:** Research agent. Fill the gaps identified in `request.md` with sourced evidence — facts, code, context, comparisons, quotes, and data.

**Input:** `.content/request.md`
**Output:** Findings appended to `.content/request.md` under the `<!-- Phase 2 -->` separator

---

## Skip gate — read first

Open `.content/request.md`. If **all** of the following are true, skip to the append step and write "None needed":
- "Known gaps" section says "None"
- User source files cover all key facts
- User said "skip research", "quick deck", or "no research needed"

Otherwise continue.

---

## Step 1 · Extract the research agenda

From `request.md`, identify:
- **Known gaps** (explicitly listed)
- **Depth level** — determines what kind of evidence to prioritise

| Depth level | Prioritise finding |
|-------------|-------------------|
| Executive | Business outcomes, risk statements, ROI data, market stats |
| Management | Trade-off comparisons, feasibility evidence, progress signals |
| Technical | Working code, benchmarks, architecture diagrams, failure modes |
| Mixed | One strong narrative hook + technical proof in separate sections |
| Async | Self-explanatory charts, step-by-step flows, full context |

Only research what is needed to fill the gaps. Do not do exhaustive research for slides whose content is already in the source files.

---

## Step 2 · Deep-read source files (if not done in Phase 1)

If source files were listed in `request.md` but not yet read, read them now:
- View folder structure if a directory was given
- Read the most relevant files
- For code repos: search key concepts, trace key functions

Extract: key facts, code patterns, architecture decisions, quotes worth featuring.

---

## Step 3 · Web research

**When:** A gap requires context, data/stats, comparisons, or validation that source files don't cover.

**Skip when:** Source files cover all gaps, or topic is internal/proprietary, or user said "skip research".

Run queries in parallel — pick the types that match the gaps:

| Gap type | Approach |
|----------|---------|
| Statistics / data | Primary or official source first; search only when URL is unknown |
| Best practices | Official docs or known authoritative articles directly |
| Case studies / examples | Search `"<topic> real-world"`, read original source |
| Comparisons | Fetch both primary docs; don't rely on second-hand summaries |
| Definitions | Official spec or standard directly |

For each finding: record the exact URL and which slide it will support.

**Ask the user only if:** a critical fact is needed and web research returned nothing reliable, and the gap would leave a slide marked `[NEEDS SOURCE]` with no alternative.

---

## Step 4 · Octocode / GitHub research

**When:** Deck needs real code samples, API references, library examples, or architecture patterns.

```
Search repos: "<topic> <language>"
Search code: "<key pattern>"
Read: README, spec, or focused source file
```

Prefer code that is credible (known org, maintained), short (≤20 lines), and directly illustrates a key point.

---

## Step 5 · Append findings to request.md

Append the findings block below the `<!-- Phase 2 -->` separator in `.content/request.md`. Do not create a new file.

```markdown
## Research findings

### From source files (additional)
{{Any new insight from deeper reading. Skip if already covered in Phase 1.}}

### From web
| URL | Key fact / quote | Supports |
|-----|-----------------|---------|
| {{url}} | {{fact}} | {{which part of deck}} |

### From GitHub
| Repo | Key finding | Link |
|------|-------------|------|
| {{owner/repo}} | {{architecture insight or code pattern}} | {{URL}} |

### Code to feature
\```{{language}}
// Source: {{URL or path:line}}
{{snippet — max 20 lines}}
\```

### Facts and data confirmed
| Claim | Source | Status |
|-------|--------|--------|
| {{stat or insight}} | {{URL or path}} | confirmed / assumed |

### Gaps still open
{{Anything that couldn't be found. If none: "None."}}
```

---

## Gate 2 — Continue immediately

Research is infrastructure, not a deliverable. Do not ask for approval. Send a one-line note and continue:

```
Research complete → {{n}} sources · {{n}} code samples · gaps: {{list or "none"}} → building outline
```

Only stop here if a gap critical to the deck's main claim cannot be filled and only the user can resolve it. State the specific gap and ask one targeted question.
