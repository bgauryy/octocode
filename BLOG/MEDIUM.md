# Medium Article Brief — Octocode Benchmark

## Goal

Plan and draft a Medium post for two audiences:

1. **Readers new to Octocode** — explain what Octocode is, what problem it solves for coding agents, and why it is different from calling `gh` or dumping repository files into a model.
2. **Readers already familiar with Octocode** — show the benchmark metrics they may not know yet, then explain the design choices behind those results.

The article should be evidence-led, technical enough for developers, and easy to read for readers who care about agent tooling, GitHub research, context-window efficiency, and benchmark design.

## Core message

Octocode is an evidence router for coding agents. Its approach is to let agents keep **full research capability** — search, exact reads, structure, LSP semantics, package lookup, pagination, minification, batching, and safety controls — but deliver that capability through efficient evidence routing instead of truncation or context bloat. In the benchmark, that design reaches near-parity correctness with `gh`, `gh + RTK`, and `gh + Headroom`, while delivering far fewer characters into the model context.

The simple takeaway: **less irrelevant context means better attention on the evidence that actually matters**. Octocode does not win by hiding capability or blindly truncating output; it wins by fetching and shaping the smallest useful proof, not by relaying whole files, whole trees, or post-compressed command output.

## Headline numbers to use

Source: `packages/octocode-benchmark/results/SUMMARY.md` and `packages/octocode-benchmark/results/PER_QUESTION_SUMMARY.md`.

- Benchmark: 30 GitHub research questions × 3 passes, blind neutral `gpt-5.5` judge, leanest-path enforced.
- Correctness: near-ceiling tie across tools.
- Character efficiency:
  - Octocode: **22,111 mean chars/question**, **663,319 total chars** over 30 questions.
  - Plain `gh`: **114,849 mean chars/question**, **3,445,482 total**, headline geo-mean ratio about **2.0× more** than Octocode.
  - `gh + RTK`: **368,586 mean chars/question**, **11,057,569 total**, headline geo-mean ratio about **3.2× more** than Octocode.
  - `gh + Headroom`: **382,598 mean chars/question**, **11,477,951 total**, headline geo-mean ratio about **2.6–2.7× more** than Octocode.
  - Important nuance: the ratio column should be described as the per-instance/per-question geometric headline ratio, not simple total-character division. Plain-English gloss: geometric mean reduces distortion from one huge outlier question.
- Published CLI validation (`npx octocode@18.2.2`) also shows correctness parity and Octocode **2.37× leaner vs RTK** and **2.27× leaner vs Headroom**.

## What was measured

Explain the benchmark clearly:

1. Each question is a self-contained GitHub research task.
2. Three isolated agents answer each question using one assigned tool path: Octocode, plain `gh`, `gh + RTK`, or `gh + Headroom` depending on matchup.
3. A fourth blind judge scores correctness, depth, workflow quality, and character usage.
4. Character count is model input + model output from per-call logs.
5. The fairness rule: every tool must use the leanest legitimate path; no whole-file or whole-tree dumps when a targeted search/read is enough.

Primary benchmark docs and public links:

- Questions list: https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark/compare/github-questions
- HTML report: https://raw.githack.com/bgauryy/octocode/main/packages/octocode-benchmark/results/index.html
  - Caveat: RawGithack shows a one-step external-content interstitial before opening the report. For publication, consider hosting the report on GitHub Pages or linking the GitHub `results/index.html` source beside it.
- Official site: https://octocode.ai
  - Caveat: `octocode.ai` and `octocode.ai/blog` respond, but the blog listing appears client-rendered to static fetchers; do not link a future individual blog post URL until it is published and tested.
- `packages/octocode-benchmark/README.md`
- `packages/octocode-benchmark/results/SUMMARY.md`
- `packages/octocode-benchmark/results/PER_QUESTION_SUMMARY.md`
- `packages/octocode-benchmark/skills/octocode-benchmark/references/BENCHMARK.md`
- `packages/octocode-benchmark/skills/octocode-benchmark/references/SCORING.md`
- `packages/octocode-benchmark/skills/octocode-benchmark/references/JUDGING.md`

## How Octocode works — design points to explain

Use these mechanisms from `.octocode/WHY_OCTOCODE_AND_HOW_TO_IMPROVE.md` and the architecture docs:

1. **One shared tool brain, two user surfaces**
   - `packages/octocode` is a thin CLI: it parses commands and renders output.
   - `packages/octocode-mcp` is a thin stdio MCP server: it owns lifecycle, tool registration, and output safety.
   - Both route into shared tool logic in `@octocodeai/octocode-tools-core` / `@octocodeai/octocode-core`, so CLI and MCP users get the same primitives.

2. **Rust/native engine for heavy lifting**
   - `@octocodeai/octocode-engine` is a napi-rs native package plus TypeScript orchestration.
   - Rust owns pure primitives: minification, local search, structural AST search, signatures, binary/text utilities, and the secret-detection/sanitizer core.
   - TypeScript orchestration owns stateful pieces such as the LSP client pool, symbol resolver, security registry, and path/command validators.

3. **Targeted evidence instead of dumps**
   - `ghSearchCode` snippets can answer directly; if the snippet contains the answer, the agent can stop.
   - `ghGetFileContent` can return a `matchString` window or `startLine/endLine` slice instead of a whole file.
   - Local tools follow the same philosophy: discover path/shape first, then read the exact region.

4. **Minification and structure-first reading**
   - `minify:"symbols"` gives agents a cheap outline of large or unknown files.
   - Standard minification removes boilerplate/noise so the model sees the important source shape first.
   - The article should explain this as "zoom out to structure, then zoom in to proof."

5. **Pagination and continuation controls**
   - Heavy results are paginated or chunked with continuation offsets.
   - Agents pay for the next page only when the current evidence is insufficient.
   - This is one reason Octocode can avoid accidental multi-megabyte context dumps.

6. **Batching for parallel research moves**
   - Tools support `queries[]`, so agents can run related independent probes together.
   - This reduces custom scripting and keeps research flows inside a documented tool contract.

7. **Safety and security cleanup at the boundary**
   - MCP output is sanitized before it reaches the model: `sanitizeCallToolResult`, `ContentSanitizer`, and masking normalize unsafe outputs/errors.
   - Secret detection covers 300+ provider patterns across cloud, AI providers, SaaS/dev tools, JWTs, PEM/SSH keys, bearer tokens, DB strings, and high-entropy values.
   - Local access uses path validation, sensitive-file blocking, symlink re-validation, and structured errors.
   - Large or unsafe content can be redacted instead of dumped.

8. **Tool-call building blocks designed for agents**
   - Tool calls are reasoning-oriented: query shapes carry research-intent fields such as `researchGoal` and `reasoning`, so agents can state why they are calling a tool instead of firing blind probes.
   - Every tool accepts bulk input with `queries[]` — up to 5 queries per call — so an agent can ask several independent questions without custom scripts or repeated round trips.
   - Responses can include machine-readable `next` maps with dynamic follow-up suggestions, such as exact fetch, standard/minified fetch, symbol skeleton fetch, LSP definition/reference lookup, or next-page continuation.
   - Minification is available out of the box and can be selected by request: `standard` for compact readable content, `symbols` for structural skeletons, and `none` for exact raw text when fidelity matters.
   - Agents can inspect code from several dimensions: folder/repo structure with `localViewStructure` or `ghViewRepoStructure`, text/regex/AST search with `localSearchCode`, targeted content with `localGetFileContent` / `ghGetFileContent`, and semantic proof with `lspGetSemantics`.
   - The intended flow is connected: view structure to understand context, search to find anchors, then fetch by `matchString`, line range, or symbol skeleton instead of reading whole files.

9. **Evidence-first research loop**
   - Octocode is built around an adaptive loop: scope the question, orient in structure, search cheaply, read exact bytes, prove with anchors or LSP, then decide.
   - The loop is not ceremony: agents should skip stages when evidence already answers the question and stop once the answer is proven.
   - It treats search snippets as candidates, fetched bytes as proof, and comments/issues/PR discussions as claims that may need confirmation against code or diffs.
   - Tool outputs are designed to chain safely: `next.*`, owner/repo, branch, PR/commit IDs, `localPath`, match ranges, and matched lines should feed the next call instead of being recomputed or guessed.
   - Pagination has distinct shapes: list pages, character-window continuation for large content, and cursors for discussion-like APIs. The article can explain this as "continue precisely, not blindly."
   - Empty or noisy results are handled adaptively: widen/narrow filters, try synonyms, switch from text search to AST/LSP, or change evidence surfaces before concluding absence.

10. **Agent-facing guidance**
   - Tool schemas, descriptions, hints, and `next` suggestions help agents choose the next cheapest proof step.
   - The article should frame this as product design for agents, not just API design for humans.

## Web-backed structure guidance

Useful patterns from Medium and technical-blog writing advice:

- Make the **audience and promise** explicit early: developers building or evaluating coding-agent research tools.
- Do not hide the result. Give the benchmark claim in the first 2–3 paragraphs, then spend the article proving it.
- Use a clear beginning/middle/end, with headings that let readers skim.
- Medium readers respond well to a story or tension, not just a report. Frame the tension as: "agents do not need bigger dumps; they need smaller, better evidence."
- Keep the main article around a focused 5–10 minute read. Push raw methodology and reproducibility details into links/appendix-style sections.
- Use visual breaks: one headline table, one flow diagram, one concrete example of a targeted read vs whole-file fetch.
- Keep paragraphs short and convert complex lists into bullets/tables; Medium readers often skim technical posts alongside many open tabs.
- Make the article specific rather than feature-listy: name the exact mechanism, show the measured number, then explain why it changes agent behavior.
- Avoid sounding like documentation or marketing copy. Use a human problem → evidence → implementation → limitation arc, and link exhaustive details instead of inlining them.
- Treat the Mermaid diagram as a draft source: polish it into an on-brand graphic before publishing if Medium rendering or visual quality is weak.

## Title and subtitle options

1. **Octocode Sends 2–3× Less Context for the Same GitHub Research Answers**
   - Subtitle: A 30-question blind benchmark shows why evidence routing beats whole-file dumping for coding agents.
2. **The Best Agent Tool Is Not the One That Dumps the Most Context**
   - Subtitle: How Octocode uses targeted search, snippets, region reads, and MCP/CLI tooling to keep research lean.
3. **Why Octocode Beats `gh` Wrappers on Context Efficiency**
   - Subtitle: Same near-ceiling correctness, far fewer characters through the model.
4. **Stop Feeding Agents Whole Repos**
   - Subtitle: A technical look at Octocode’s benchmark against `gh`, RTK, and Headroom.

Recommended: use option 1 for the clearest benchmark-driven headline, or option 2 for a more opinionated Medium-style hook.

## Planned post structure

### 1. Title, subtitle, and reader promise

Open with a title/subtitle that works for both audiences:

- New readers should understand that Octocode is a research tool for coding agents.
- Existing users should immediately see that this post contains benchmark numbers and design reasoning.

Recommended title:

**Octocode Sends 2–3× Less Context for the Same GitHub Research Answers**

Subtitle:

**A 30-question blind benchmark shows why evidence routing, Rust-powered code intelligence, minification, pagination, and safety cleanup matter for AI coding agents.**

### 2. Cold open: the problem before the product

Start with the bottleneck:

> Coding agents do not fail only because they lack context. They also fail because we flood them with the wrong context: whole files, whole trees, noisy command output, and transcripts that bury the one line of evidence that matters.

Then define Octocode in one newcomer-friendly paragraph:

> Octocode is a CLI and MCP tool suite that helps AI agents research codebases and GitHub repositories by searching, reading exact regions, inspecting structure, using LSP semantics, and returning evidence with anchors instead of dumping everything into the model.

### 3. Fast primer: what Octocode is

This section is for readers unfamiliar with Octocode. Keep it short and concrete:

- It can run as an MCP server for agent clients or as a CLI.
- It exposes GitHub, local-code, LSP, and npm/package research tools.
- It is designed for agents, so tool descriptions, schemas, hints, pagination, and continuation paths are part of the product.
- It is not just a wrapper around `gh`; it is a research/evidence transport layer.

### 4. Early reveal: the benchmark result

Show the main table near the top so existing users quickly get the metrics.

| Tool | Mean chars/question | Total chars over 30 Q | Headline geo-mean ratio vs Octocode | Correctness |
|---|---:|---:|---:|---|
| Octocode | 22,111 | 663,319 | 1.00× | near-ceiling |
| plain `gh` | 114,849 | 3,445,482 | ~2.0× more | near-parity |
| `gh + RTK` | 368,586 | 11,057,569 | ~3.2× more | near-parity |
| `gh + Headroom` | 382,598 | 11,477,951 | ~2.6–2.7× more | lower/near-parity depending run |

Add a one-sentence gloss below the table: "The ratio is a geometric mean, which reduces distortion from one unusually huge question while still showing the typical context gap."

Interpretation:

- Correctness mostly ties.
- Octocode’s advantage is context efficiency.
- Less irrelevant context gives the model a better chance to attend to the load-bearing evidence.

### 5. What the benchmark actually tested

Trust-building section:

- 30 GitHub research questions.
- 3 passes.
- Isolated agents.
- Blind judge.
- Leanest-path rule for all tools.
  - Add the operational definition: targeted search/read paths were required when they could answer the question, and whole-file/tree dumps were disallowed when a narrower snippet, line range, selected patch, or metadata read was sufficient.
- Character count = model input + model output.
- Public question list and HTML report are linked for reproducibility.

Also state what it does **not** test: full product capability, latency, or exact monetary cost.

### 6. Why fewer characters matter

Translate metrics into agent behavior:

- Less context pollution.
- Better attention: fewer irrelevant tokens compete with the evidence.
- Lower token/cost pressure.
- Shorter, more inspectable research trails.
- Fewer brittle "model scans a wall of output" moments.

Core line:

> The benchmark is not saying "shorter is always better." It is saying that when correctness is tied, the tool that delivers less irrelevant context gives the model a cleaner reasoning surface.

### 7. How Octocode works: evidence routing

Use this loop as the mental model:

```text
Scope → orient → search → read exact bytes → prove with anchors/LSP → decide/stop
```

Explain the key mechanisms:

- The agent keeps the full toolset, but each step asks for the smallest useful evidence.
- The loop is adaptive: start at the cheapest surface that can answer the question, skip unnecessary stages, and stop when evidence is sufficient.
- Search snippets can answer directly, but snippets are usually candidates; fetched bytes and semantic anchors are stronger proof.
- `matchString` and `startLine/endLine` reads avoid whole-file fetches.
- Structure-first reads let agents zoom out before zooming in.
- Pagination continues only when the current slice is insufficient, instead of truncating away needed information or dumping everything at once.
- Chain tool output literally — `next` suggestions, match ranges, owner/repo, branch, commit/PR IDs, and `localPath` — rather than guessing the next query.
- LSP semantics help prove definitions/references/call flow when local code identity matters.
- `queries[]` supports batched independent probes.
- Empty results trigger adaptation, not immediate absence claims: change scope, filters, wording, or evidence surface.

### 8. Under the hood: why the design produces the metric

This is the main technical section. Split it into short subsections.

#### Shared CLI/MCP brain

- CLI and MCP are thin surfaces.
- Tool logic, schemas, execution, pagination, hints, security, credentials, config, and session state live in tools-core/core.
- Result: the same behavior is available to terminal users and MCP agent clients.

#### Rust/native engine

- `@octocodeai/octocode-engine` is a napi-rs native package with TypeScript orchestration.
- Rust owns fast/pure primitives: minification, local search, structural AST search, signatures, binary/text utilities, and secret detection/sanitizer core.
- TypeScript owns stateful orchestration: LSP client pool, symbol resolver, security registry, path/command validators.

#### Tool-call building blocks

- Tool schemas include intent fields like `researchGoal` and `reasoning`, making each call explainable.
- Bulk `queries[]` lets one tool call carry several independent probes.
- Responses include dynamic `next` suggestions for common follow-ups: fetch exact, fetch standard/minified, fetch symbols, LSP lookup, or continue pagination.
- Structure view, code search, targeted fetch, and LSP are separate dimensions that compose into one research loop.

#### Minification and structure-first reading

- `minify:"symbols"` gives an outline for large files.
- Standard minification strips boilerplate and keeps the useful source shape.
- Article metaphor: "zoom out to find the right room; zoom in to inspect the exact object."

#### Pagination and continuation

- Heavy results are paginated/chunked.
- Agents only request the next page if the current page is insufficient.
- This prevents accidental context explosions.

#### Security cleanup and safe output

- MCP wraps tool output with sanitization before it reaches the model.
- Secret masking/redaction handles cloud, AI provider, SaaS/dev-tool, JWT, PEM/SSH, bearer-token, DB-string, and high-entropy patterns.
- Local reads use path validation, sensitive-file blocklists, symlink re-validation, and structured errors.
- Unsafe or oversized content can be redacted instead of dumped.

### 9. Concrete example: bad path vs Octocode path

Use a narrative example to make the design tangible:

- Bad path: search → raw fetch a full file/tree → model scans a large output blob.
- Octocode path: search snippet or path → inspect structure if needed → targeted region read → answer with source anchor.

Then connect to Headroom:

> Compression helps after excess bytes have already been fetched. Octocode wins when it avoids fetching those bytes in the first place.

### 10. Honest limitations and next improvements

This increases credibility:

- Tiny single-hit `gh` lookups can beat Octocode because fixed response overhead dominates.
- Failed/empty first probes waste characters.
- Structured-file exactness must remain deterministic.
- Improvement directions: lean response mode, better first-query guidance, exact structured reads, intra-question cache, auto-region selection.

### 11. Close with the takeaway and CTA

End with a memorable line:

> The future of agent research is not bigger context dumps. It is smaller, sharper evidence.

CTA options:

- Try Octocode through MCP or CLI at https://octocode.ai.
- Read the interactive HTML benchmark report.
- Inspect the public question list.
- Re-run or adapt the benchmark for your own agent stack.

## Suggested visuals

1. **Headline benchmark table** — chars/question, total chars, ratio, correctness.
2. **Newcomer diagram** — `Agent → Octocode CLI/MCP → GitHub/local/LSP/npm evidence`.
3. **Evidence-routing diagram** — `Search → Structure/Snippet → Targeted read → Anchor/LSP proof → Answer`.
4. **Context waste comparison** — whole-file/tree dump vs `matchString`/line-range read.
5. **Architecture mini-diagram** — CLI and MCP as thin surfaces over tools-core/core, with Rust/napi engine for minify/search/AST/security.
6. **Safety boundary diagram** — tool output → sanitizer/secret masking/path checks → model-safe result.
7. **Tool-call anatomy diagram** — `reasoning/researchGoal + queries[] + minify mode + next suggestions`.

### Mermaid diagram to include

Use this as the main technical visual. It shows the full Octocode approach: full agent capability, but routed through lean evidence instead of truncation or context bloat. Before publishing on Medium, export/polish this Mermaid source as a branded PNG/SVG because standard Medium rendering may not support Mermaid directly.

```mermaid
flowchart TD
    A[AI coding agent] --> B{Use Octocode via}
    B --> CLI[CLI]
    B --> MCP[MCP server]

    CLI --> CORE[Shared tool brain\ntools-core + octocode-core]
    MCP --> CORE

    CORE --> SCHEMA[Reasoning-oriented schemas\nresearchGoal + reasoning]
    CORE --> BULK[Bulk queries[]\nup to 5 probes/call]
    CORE --> NEXT[Dynamic next suggestions\nfetch exact · fetch standard · symbols · LSP · next page]

    CORE --> GH[GitHub tools\nsearch · structure · fetch]
    CORE --> LOCAL[Local tools\nstructure · search · find · fetch]
    CORE --> LSP[LSP semantics\ndefinitions · references · callers]
    CORE --> NPM[npm/package lookup]

    GH --> STRUCT[View structure\nrepo/folder shape]
    LOCAL --> STRUCT
    GH --> SEARCH[Search snippets\ntext/regex/code]
    LOCAL --> SEARCH
    SEARCH --> FETCH[Targeted fetch\nmatchString · line range · char page]
    STRUCT --> FETCH
    FETCH --> MINIFY[Minify by request\nstandard · symbols · none]
    FETCH --> PROOF[Anchored proof\npath · line · snippet · symbol]
    LSP --> PROOF

    CORE --> ENGINE[Rust/napi engine\nminify · search · AST · signatures · secrets]
    ENGINE --> MINIFY
    ENGINE --> SAFE[Security cleanup\nsecret masking · path checks · redaction]

    MINIFY --> SAFE
    PROOF --> SAFE
    SAFE --> OUT[Lean model context\nfull capability, less bloat]
    OUT --> ATTENTION[Better attention\non load-bearing evidence]
```

## Tone

- Confident but not hype-heavy.
- Be transparent about scope: this benchmark measures research-answer quality and character efficiency, not total product capability, latency, or monetary cost.
- Mention that public benchmark results are orientation; private held-out tasks are better for shipping decisions.
- Avoid claiming Octocode always wins. It can lose on tiny single-hit lookups where one `gh` command already answers.

## Drafting prompt

Use the information above to write a polished Medium article for readers who may not know Octocode and readers who know it but have not seen the metrics. Start by explaining the problem and what Octocode is, reveal the benchmark numbers early, include the Mermaid diagram as the main technical visual, then explain how the design produces the result: shared CLI/MCP tool brain, Rust/native engine, reasoning-oriented tool calls, bulk `queries[]`, dynamic `next` suggestions, structure view, code search, targeted `matchString`/line-range fetches, minification modes (`standard`, `symbols`, `none`), pagination, batching, LSP proof, and security cleanup. Keep the article grounded in the cited repo files and avoid unsupported marketing claims.
