# Evidence-Graded Retrieval for Agentic Code Research: A Routing Model and Three Reading Dimensions

**Status.** Position paper draft. Extracted from an internal engineering manifest
(`OCTOCODE_RESEARCH_MANIFEST.md`) and stripped of implementation-specific
operational detail so the model stands on its own. The claims below are
qualitative and framework-level; §5 states plainly what has and has not been
measured.

## Abstract

Agentic code research fails in two symmetric ways: an LLM reasoning without
deterministic backing hallucinates identifiers and generalizes from a single
match; deterministic retrieval tools without an agent directing them cannot
decide what the question even is. We describe a routing model for the middle
ground — pairing LLM judgment with retrieval primitives of known, differing
reliability — built around two ideas. First, **evidence grading**: lexical
(text/regex), structural (AST-matched), semantic (language-server-proven),
and provider-index retrieval each prove different things and fail silently in
different ways, so a routing decision should be made by *what grade of
evidence a question requires*, not by a fixed tool pipeline. Second, **three
reading dimensions**: any codebase question can be approached through
structure (the file tree), stream (raw or outlined text), and connections
(call/import/type graphs), and each dimension is individually incomplete and
silent about what it cannot see. We argue that agent quality on code-research
tasks is more sensitive to reading across dimensions than to going deeper in
one, and that cross-dimension *disagreement* is itself informative rather
than noise to average away. The model is implemented and exercised in an
open multi-tool retrieval surface, but this paper makes no task-level
benchmark claim; §5 states that gap directly rather than implying it is
closed.

## 1. Motivation

Two independent failure modes recur in agentic code research:

- **Ungrounded reasoning.** An LLM answering from memory or a single grep
  hit will hallucinate line numbers, overgeneralize from one match, and
  cannot distinguish a real call site from a comment or a same-named symbol
  in an unrelated scope.
- **Undirected tooling.** Deterministic tools (grep, AST matchers, language
  servers, provider search indexes) are individually reliable within a
  narrow scope, but none of them can decide *what question is being asked*
  or *when the evidence gathered so far is sufficient*. That judgment call
  is exactly what an LLM is suited for.

The practical question this paper addresses is not "which single tool is
best" but: given a set of retrieval primitives with different evidentiary
strength, how should an agent decide which to reach for, when to stop, and
when to distrust its own conclusion.

## 2. Evidence grades

We classify retrieval results into four grades, ordered by what they prove
versus what they miss:

| Grade | Proves | Blind to |
|---|---|---|
| **Semantic** (language server: definitions, references, call hierarchy) | Proven identity, scoped to the language-server's project graph | Scripts, string-based/dynamic wiring, re-exports treated as plain text, non-project files |
| **Structural** (AST match with capture variables) | Proven shape: a match is a complete syntax node, not a substring | Anything outside the pattern's shape; partial patterns silently return zero rather than "close" matches |
| **Lexical** (text/regex search) | Total coverage — sees every occurrence, including outside the language server's scope | Cannot distinguish a call from a comment, string, or shadowed name; proves nothing about identity |
| **Provider** (a hosted code-search index) | Fast, cross-repository reach | Typically default-branch-only; unindexed or archived sources return a false negative indistinguishable from a true one |

The central methodological claim is a single rule: **never conclude a claim
about impact, usage, or absence from one grade alone.** Each grade fails
silently in a different place, and a grade's *zero result* is a claim about
that grade's coverage, not about the codebase. A semantic call-hierarchy
query returning zero callers is a statement about what the language server's
project graph can see, not proof the symbol is unused — a lexical pass can
surface a call site the language server structurally cannot reach (e.g.,
invoked through a dispatch table, or from a script outside the project).
Conversely a lexical hit is not proof of a real reference until corroborated,
because grep cannot separate a call from a string or a comment. Treating
*disagreement between grades* as the deliverable, rather than picking
whichever grade answered first, is the mechanism that catches this class of
error before it becomes a wrong conclusion.

## 3. Three reading dimensions

Orthogonal to evidence grading is a second axis: what *aspect* of the
codebase a query targets. We identify three:

- **Structure** — the file tree: layout, naming, nesting, size. Answers
  "where does this live, relative to what."
- **Stream** — the text itself, exact or outlined (e.g., a signatures-only
  view). Answers "what is actually written."
- **Connections** — the call/import/type graph, proven either by a language
  server or by structural pattern matching. Answers "what refers to what."

Each dimension alone gives a systematically partial model, and none of them
flags that it is partial:

- Structure gives layout with no meaning — a plausibly-named file proves
  nothing about its actual behavior or its relationship to its neighbors.
- Stream gives coverage with no proof — it sees every occurrence of a string
  but cannot tell a genuine reference from an unrelated one.
- Connections give proof with a scope boundary — capability gaps (an
  unsupported language, dynamic dispatch, a non-project script) are
  invisible to it, and a language server that cannot see something reports
  a capability limitation, not a proof of absence, though the two are easy
  to conflate if the caller does not check for it explicitly.

We conjecture — without yet having measured it directly, see §5 — that
research quality on non-trivial code questions correlates more strongly with
*how many of the three dimensions were read* than with how deep any single
dimension was queried. An agent that only ever searches text, or only ever
queries a language server, builds a model that is confidently incomplete in
a way it cannot detect from within that one dimension. The mitigation is
procedural rather than architectural: before concluding on a non-trivial
question, deliberately pull at least two of the three dimensions, and treat
a mismatch between them (e.g., structure suggests a module is a leaf, but a
connections query finds an inbound edge from outside that module) as a
signal to look closer, not as noise.

## 4. A routing procedure

The model above motivates routing by *what the agent already holds*, rather
than a fixed pipeline (e.g., always grep before AST before semantic). If the
agent already holds a concrete identifier, going straight to a semantic
query is both cheaper and higher-grade than starting with a lexical scan; if
the agent holds only a natural-language concept with no identifier, no
semantic query is possible until a lexical or structural pass produces one.
In outline:

```
No prior orientation           → read structure first (tree), then a coarse
                                  stream pass (e.g. a hit-count-per-file scan)
                                  to find where the concern concentrates
A concept, no identifier       → lexical/synonym search over stream, then
                                  narrow to an outlined view of the winning file
A concrete identifier          → go directly to a connections query (semantic
                                  lookup); use stream only to cross-check
A shape ("all calls that do Y") → a structural query with captures
A claim about impact/usage      → never single-grade; a connections query
   or absence                    plus an independent stream-level cross-check,
                                  and treat any disagreement as the finding
```

This is a decision procedure, not a novel primitive — each individual step
(grep, AST match, language-server query, tree listing) is standard tooling.
The contribution is the routing discipline layered on top: choosing entry
point by what is already known, grading what comes back, and treating
cross-dimension or cross-grade disagreement as signal rather than an
inconvenience to resolve by picking one answer.

## 5. Related work and open gaps

**Where this model aligns with recent context-engineering work.** The
practice of holding a lightweight handle and fetching content only on demand
matches "just-in-time retrieval" as described in industry context-engineering
guidance (Anthropic, 2025; a comparable "memory pointer" pattern is described
independently by StackOne, 2025-2026). Tiered, on-demand schema disclosure —
a small catalog before a large per-tool contract — mirrors StackOne's
"tool-definition catch-22" framing. The qualitative ordering of evidence
grades (semantic > structural > lexical > provider) is consistent in
direction with Sourcegraph's CodeScaleBench results, which report
substantially higher file recall and precision for a code-graph-augmented
retrieval layer over grep-only baselines; this paper's ordering is argued
qualitatively and has not been validated against that or any other
task-level benchmark under our own model (see gaps below).

**Deliberately excluded lanes.** Two retrieval families are intentionally out
of scope: embedding/vector indexes (fuzzy concept recall over large unfamiliar
corpora, at the cost of index staleness and identity-blindness), and
precomputed knowledge/code graphs (cheaper at very large monorepo scale, at
the cost of being memoryless of point-in-time query intent). Both are
plausible complements to, not replacements for, the model above, and the
routing procedure in §4 explicitly names where each would outperform it
(purely conceptual queries at large scale for the former; repo-wide blast-
radius sweeps at scale for the latter).

**Adjacent, not addressed here.** Compaction of an agent's own conversation
history and sub-agent isolation ("code mode") are harness-level concerns
belonging to the calling agent, not the retrieval layer described here; they
are complementary rather than competing.

**Open gaps, stated directly rather than implied away:**

1. **No task-level benchmark exists for this routing model against a
   grep-only or single-lane baseline.** Every claim above about relative
   quality is qualitative and directional, not measured. This is the single
   largest gap between this paper and a comparable systems-evaluation paper
   such as Sourcegraph's CodeScaleBench work.
2. **The cross-grade/cross-dimension check is agent discipline, not a
   tool-enforced guarantee.** Nothing in the model prevents an agent from
   skipping the second dimension; the claim is only that skipping it
   produces a worse and undetectably-partial result, not that the tooling
   makes skipping impossible.
3. **Scale is untested.** The model has not been validated on very large
   monorepos, where a precomputed graph is plausibly a better entry point
   than per-query language-server calls.
4. **"Three dimensions" is a descriptive framework, not a metric.** We do
   not yet have a way to quantify "how many dimensions were read" as a
   feature that predicts task success; this is future work, not a claim
   made here.

## 6. Conclusion

We describe a routing model for agentic code research built on two axes:
evidence grading (what a retrieval result actually proves, versus what grade
of failure it produces silently) and reading dimension (structure, stream,
connections — each individually incomplete). The procedural claim is narrow
and testable in principle: route by what the agent already holds, grade
results by what they prove, and treat disagreement across grades or
dimensions as a finding rather than noise. The honest limit of this paper is
equally narrow: this is a description of a procedure argued qualitatively
against related work, not a measured outcome, and closing that gap with a
task-level benchmark is the clearest next step.

## References

- Anthropic, "Effective context engineering for AI agents," 2025.
  https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Sourcegraph, "Context engineering for coding agents," 2026.
  https://sourcegraph.com/blog/context-engineering
- Yang et al., "SWE-agent: Agent-Computer Interfaces Enable Automated
  Software Engineering," arXiv:2405.15793, 2024.
  https://arxiv.org/abs/2405.15793
- Aider, "Repository map," documentation.
  https://aider.chat/docs/repomap.md
- "OpenDev: anchor-based retrieval and layered LSP for coding agents,"
  arXiv:2603.05344, 2026. https://arxiv.org/html/2603.05344v1
