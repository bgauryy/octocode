# Question Bank

Shared research questions used across benchmarks. A question file is a target-specific set of research tasks that any agent can attempt using its own tooling.

---

## Files

| File | Target repo | Type | Count |
|---|---|---|---|
| [`nextjs.md`](./nextjs.md) | `vercel/next.js` | External (Q1–Q10) + Local clone + LSP (Q11–Q20) | 20 |

Used by:
- future full comparison runs — three-way or four-way comparisons such as `octocode · gh · rtk`
- ad hoc local validation of tool coverage across GitHub, local clone, package, and LSP-style research

Per-benchmark question sets (`benchmark/github/QUESTIONS.md`, `benchmark/rtk/QUESTIONS.md`) are kept in their own directories for historical runs.

---

## Question Format

Each question file is a Markdown file with numbered questions. Follow this format exactly — the metering scripts count questions from headings to set `.q-count`.

### Required structure

```markdown
# Questions

<short description>. N research questions about `<owner>/<repo>`.

**<optional grouping note, e.g. "External questions (Q1–Q10)">**

---

### Q1 — <Title> `[CATEGORY]` `[drift]`?

<Question body — exactly what the agent must answer. Use numbered sub-questions when asking for multiple facts.>

> *<Evaluator note explaining what capability this question probes and what to look for when scoring.>*

---
```

### Category tags

Every question must carry one category tag in its heading. Tags are used in the per-question table of `summary.md`.

| Tag | What it tests |
|---|---|
| `[SEARCH]` | Code search completeness, bulk multi-query, AND-intersection |
| `[CONTENT]` | File content retrieval — large files, targeted reads, pagination |
| `[STRUCTURE]` | Directory/tree navigation, subtree enumeration |
| `[PR]` | PR metadata, inline comments, commits, diff access |
| `[REPOS]` | Repository search, filters, pagination |
| `[LOCAL]` | Local filesystem search, ripgrep, find |
| `[METADATA]` | File/repository metadata such as size, recency, timestamps, counts |
| `[LSP]` | Symbol definition, references, call hierarchy |
| `[PACKAGE]` | npm registry lookup |

### Drift tag

Add `[drift]` to any question whose answer will change over time (star counts, recent PR lists, current versions). The judge scores drift questions loosely and reports them in a separate section.

### Evaluator notes

Every question should have a `> *...*` evaluator note below the body. This note:
- Names the specific capability being probed
- States what the judge should check (e.g. "judge verifies line numbers are correct")
- Flags any known traps (e.g. "file is >1MB — tests over-size-limit retrieval")

Do not put expected answers in the evaluator note. The judge independently verifies.

---

## Scoring Axes

Questions are scored on three axes by the judge (see [`../judge/prompt.md`](../judge/prompt.md)):

**Quality `Q` (0–3)** — factual accuracy
**Depth `D` (0–3)** — research thoroughness and citation quality
**Turns `T`** — tool invocation count (from `calls` in per-Q JSON)

The composite `tradeoff_score = (Q × D) / (effective_chars / 1000)` is the winner axis.

`effective_chars` is the deterministic token-budget ruler: per-question `in_chars + out_chars`, plus any explicitly recorded and amortized init cost. `approx_tokens = ceil(effective_chars / 4)` is display-only unless all compared agents also provide actual model token counters.

---

## Designing Good Questions

**Probe a real capability gap.** The best questions have different answers depending on whether the tool can do something — not just different efficiency. Example: a question requiring inline PR review comments is unanswerable with `gh pr view --json reviews`.

**One clear answer, independently verifiable.** The judge must be able to fact-check the answer without ambiguity. Avoid questions whose answers require subjective interpretation.

**Reward depth.** Sub-questions and cross-references reward research depth. A question like "find the PR, quote the reviewer objection, and identify the file it targeted" requires three research steps — shallow agents miss the last two.

**Mark drift.** Any question that depends on real-time GitHub state (issue counts, latest release, current stars) gets `[drift]` and is scored loosely.

**Include both external and local questions** (when appropriate for the benchmark). External questions (GitHub API) and local questions (clone + LSP) test different tool capabilities.

---

## Adding a New Question Set

1. Create `benchmark/questions/<topic>.md` following the format above.
2. Add it to the table at the top of this file.
3. Reference it in the benchmark README(s) that will use it.
4. Update `.q-count` logic if the benchmark's `init-run.sh` reads from a different questions path.
