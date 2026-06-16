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

If source files were listed in `request.md` but not yet fully read, go deeper now. Use the same tool routing as Phase 1 Step 3:

| Source type | Deeper research approach |
|-------------|-------------------------|
| Local workspace / folder | `localSearchCode` for key concepts and patterns → `localGetFileContent` on specific files → `lspGotoDefinition` to trace a function to its definition → `lspFindReferences` to see where a type/function is used → `lspCallHierarchy` to trace call chains (for code-flow slides) |
| GitHub repo | `ghSearchCode` for key API patterns or architecture → `ghGetFileContent` for specific files/sections → `ghSearchPRs` for context on design decisions |
| Package / library | `npmSearch` → check deprecation, repo URL → dive into repo with GitHub tools |

For code repos: search key concepts, trace function call chains with LSP tools, and read 3–5 files that directly support the slide content. Extract: real code patterns (≤20 lines), architecture decisions, API signatures, error handling examples, benchmarks.

Do not read all files indiscriminately — stay focused on what fills the gaps in `request.md → Known gaps`.

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

## Step 4 · Octocode research — external code, repos, and packages

**When:** Deck needs real code samples, API references, real-world architecture patterns, or library usage examples that aren't in the user's source files.

**Tool routing by goal:**

| Goal | Tool chain |
|------|-----------|
| Find repos for a topic | `ghSearchCode(match="path", keywords)` → pick 1-2 credible results → `ghViewRepoStructure` → `ghGetFileContent` |
| Find a code pattern in a known repo | `ghSearchCode(match="file", owner, repo, keywords)` → `ghGetFileContent(matchString=...)` for the relevant section |
| Research a library / package | `npmSearch(name)` → check repo URL → `ghViewRepoStructure` → read README + key source files |
| Trace how a function is used | Clone repo locally with `ghCloneRepo` → `localSearchCode` → `lspFindReferences` or `lspCallHierarchy` |
| Find real-world examples of a pattern | `ghSearchCode(match="file", keywords=["pattern", "example"])` across GitHub |

**Code quality criteria — feature only code that is:**
- From a credible, maintained repo (known org, recent commits, reasonable star count)
- Short enough to read on a slide (≤20 lines; trim setup/imports unless they're the point)
- Directly illustrating the claim in the slide title — not surrounding scaffolding
- Real, not paraphrased or invented from memory

Record each code snippet with its exact source URL and line range in the `Code to feature` section (Step 5).

---

## Step 5 · Append findings to request.md

Append the findings block below the `<!-- Phase 2 -->` separator in `.content/request.md`. Do not create a new file.

```markdown
## Research findings

### From local workspace
{{Key findings from local source files not already captured in Phase 1. Include file path and relevant lines.}}
| Path:lines | Finding | Supports |
|------------|---------|---------|
| {{path:L12-28}} | {{architecture decision / API shape / key pattern}} | {{which slide}} |

### From web
| URL | Key fact / quote | Supports |
|-----|-----------------|---------|
| {{url}} | {{fact}} | {{which part of deck}} |

### From GitHub / external repos
| Repo | Key finding | Link |
|------|-------------|------|
| {{owner/repo}} | {{architecture insight or code pattern}} | {{URL#L}} |

### Code to feature
\```{{language}}
// Source: {{full URL with line range, or local path:L12-28}}
{{snippet — max 20 lines, trimmed to the point}}
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
