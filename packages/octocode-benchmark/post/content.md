# The Octocode Benchmark: Context Engineering for Leaner GitHub Research

![octocode.ai](./assets/octocode-logo-official.png)

> **TL;DR**
>
> Coding agents need evidence beyond the open repo, but dumping everything into a model creates noise. Octocode follows an evidence-first loop: orient, search, read the exact slice, prove, and continue only when needed.
>
> In a 30-question GitHub research benchmark, Octocode stayed in the same high correctness tier as plain `gh`, `gh + Headroom`, and `gh + RTK` while using about **50–70% less context** — roughly **half to one-third as many characters through the model**.
>
> Shorter context helps only when it stays precise enough to preserve correctness.

## Why coding agents need GitHub research

The repo open in an editor is rarely the whole story. A bug may live in an upstream dependency. An undocumented behavior may have landed in an old commit. A strange guard may only be explained in a pull-request review. A local service may implement a contract owned in another repository.

Without those connections, an agent is forced to guess from whatever is in front of it. Good agents do not guess—they gather proof. Before changing code, they need to understand how the system works and build a clear picture of the relevant context.

That is true for open source and for internal work. Building on a framework like LangChain means checking current APIs and patterns in the source, not relying on stale training data. Inside an organization, dependencies and contracts often span many repositories, shared schemas, and years of review history. Agents need an efficient way to pull that cross-repo context without flooding the model.

Large context windows raise capacity, but capacity is not attention. Too much payload buries the answer. Too little, with no way to continue, forces a guess. **Context is both storage and an attention budget.**

## Dump-first vs. evidence-first research

Ask an agent: *“Which configuration field controls this behavior, and what is its value?”*

A dump-first path searches broadly, fetches a whole file or tree, and hopes the model finds the answer inside the noise.

An evidence-first path does the opposite: map the question, search for candidates, read a narrow slice, prove the claim with anchors, and stop when the evidence is enough—or refine and loop if it is not.

![Dump-first vs evidence-first: SCOPE → ORIENT → SEARCH → READ EXACT → PROVE → DECIDE, with refine-or-answer at the end](./assets/evidence-routing.png)

The loop is simple:

> **SCOPE → ORIENT → SEARCH → READ EXACT → PROVE → DECIDE**

Empty or noisy results trigger refinement—not an absence claim. The goal is not “less context at any cost.” It is the cheapest useful evidence that still preserves correctness.

## What Octocode gives the agent

Octocode is a CLI and MCP suite for evidence-first research across GitHub, npm, and checked-out code. The same loop maps onto concrete tools:

1. **Scope** — find the likely repo or package (`ghSearchRepos`, `npmSearch`)
2. **Orient** — map the relevant tree (`ghViewRepoStructure`)
3. **Search** — surface candidates in code, commits, or PRs
4. **Read exact** — fetch a matching window, line range, or bounded directory
5. **Prove** — confirm with local search and LSP semantics (definitions, references, callers)
6. **Decide** — use pagination cursors and typed `next` hints to continue, or stop when proven

Plain `gh` and wrappers like RTK or Headroom can still produce correct answers. They often reshape output *after* retrieval. Octocode tries to control context *earlier*—at search and region-read boundaries—while keeping a clear path to request more.

## The benchmark

Feeling lean is not the same as working. We tested whether evidence routing could keep answer quality high while cutting the characters delivered to the model.

![Benchmark hero: Octocode used about 50–70% less context with comparable correctness](./assets/octocode-benchmark-hero.png)

We ran a public 30-question suite, three pairwise matchups (Octocode vs plain `gh`, `gh + Headroom`, `gh + RTK`), and three passes each. Correctness came first: a shorter wrong answer never beats a longer correct one.

### Results

Efficiency is the geometric mean of per-question character ratios (baseline ÷ Octocode), so one multi-megabyte outlier cannot dominate.

| Matchup | Correctness (Octocode / baseline) | Baseline characters vs Octocode |
|---|---|---|
| Octocode vs plain `gh` | 9.19 / 9.27 — near parity | **1.99×** (95% CI 1.52–2.61) |
| Octocode vs `gh + Headroom` | 9.30 / 8.62 — Octocode edges | **2.62×** (95% CI 1.87–3.71) |
| Octocode vs `gh + RTK` | 9.29 / 9.42 — near parity | **3.21×** (95% CI 2.36–4.46) |

![Results chart: bars grow right of the Octocode 1× line; correctness labeled Octocode / baseline; all 95% CIs above 1×](./assets/benchmark-results.png)

*Bars show baseline characters ÷ Octocode characters. Right of the `1×` line means the baseline delivered more context. Characters are not tokens.*

All three matchups stayed in the same high correctness tier, and every character-ratio confidence interval was above 1×.

A separate 30×3 validation on published `octocode@18.2.2` showed the same direction: **2.37× leaner than RTK** and **2.27× leaner than Headroom**, with correctness parity.

### How we kept the comparison fair

1. **Isolated runners** — one agent with Octocode, one with the baseline, same question.
2. **Leanest-path rule** — each arm used its leanest legitimate path; metadata or targeted search was enough when it answered the question.
3. **Blind judging** — a neutral model scored answers labeled only “Agent X” and “Agent Y,” against ground truth.
4. **Character accounting** — Unicode characters to and from the model. This is **not** tokens, latency, or dollar cost.

To run that comparison correctly between agents—fresh isolated runners per question and tool arm, plus a blind judge—we published the [`octocode-benchmark` skill](https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark/skills/octocode-benchmark). It encodes the preflight, spawn packets, leanest-path rules, and character measurement so others can re-run or adapt the suite.

## How Octocode controls context

CLI and MCP are thin fronts over one shared path: receive the goal, validate and secure the request, run against GitHub / npm / local code, then return a bounded result with continuation hints.

![Left-to-right request path: Goal → CLI/MCP → tools-core → Validate & secure → Provider/engine → Bounded result](./assets/octocode-request-flow.png)

Under the hood, remote providers fetch GitHub and npm evidence. A Rust/napi engine handles local search, minification, structural analysis, and LSP. Caching avoids repeat fetches; line-aware pages and security sanitization keep payloads lean and safe.

![Architecture: CLI and MCP into shared tools-core, with arrows to metadata, remote evidence, and native engine, plus layered safety](./assets/octocode-architecture.png)

## Octocode context-engineering main approach

### 1. Orient before reading

When the location is unclear, map the tree before opening plausible files. Zoom out, then zoom in.

### 2. Treat search hits as candidates

A snippet can answer a narrow question, but it is not always proof. Use it to find the exact region—or the commit / PR—that can support the claim. Rank candidates against the question before the next tool call; do not treat the first hit as the answer.

### 3. Match the read to the question

Use a symbol outline when only structure matters, compact code for most investigation, and exact raw text for edits, quotes, or config values. Narrow regions also reduce ["Lost in the Middle"](https://arxiv.org/abs/2307.03172) effects. Octocode resolves GitHub search hits to real line ranges so the model gets the relevant slice, not a floating fragment.

### 4. Make continuation cheap and deterministic

When more evidence is needed, pagination and typed next-step hints let the agent continue without reconstructing the query or re-sending earlier content. Independent probes can run together in one batch.

### 5. Force reasoned, research-oriented tool use

Do not let the model jump from a vague goal to a tool dump. Require an explicit step before each search or read: state the hypothesis, what evidence would confirm or refute it, and which tool/query is the cheapest probe. Keep the agent in a research loop—scope, candidate search, exact read, prove—so tool calls stay purposeful instead of exploratory thrashing.

## What this benchmark does not prove

This is a public 30-question GitHub research suite run by the Octocode project—transparent project evidence, not third-party certification.

It does not measure latency, monetary cost, private held-out tasks, or every agent workflow. Tiny single-hit `gh` lookups can still be leaner when Octocode’s structured response overhead dominates. Failed first probes still waste characters.

That points to clear next steps: a leaner response mode, better first-query guidance, tighter structured reads, and smarter automatic region selection.

## The takeaway

On this suite, Octocode stayed in the same high correctness tier while using roughly **50–70% less context** than the three baselines.

Bigger windows expand what agents can attempt. They do not remove the need to choose what evidence belongs in that window. Shorter is not automatically better. **Sharper evidence is better when it preserves correctness.**

**Explore the evidence:**

- [octocode.ai](https://octocode.ai)
- [Benchmark skill](https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark/skills/octocode-benchmark)
- [Benchmark questions](https://github.com/bgauryy/octocode/tree/main/packages/octocode-benchmark/compare/github-questions)
- [Benchmark report](https://raw.githack.com/bgauryy/octocode/main/packages/octocode-benchmark/results/index.html)
- [Research Driven Development Manifest](https://github.com/bgauryy/octocode/blob/main/MANIFEST.md)

If you liked this post, feel free to explore — and add a ⭐ to the [Octocode repo](https://github.com/bgauryy/octocode) 🐙
