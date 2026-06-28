---
name: octocode-brainstorming
description: Use when the user wants to brainstorm or validate an idea grounded in evidence — triggers like "brainstorm", "is this worth building", "has anyone built X", "validate my idea", "check if X exists", "research this idea", "prior-art options for Y", "should we add X to our app/codebase". Researches the local workspace (when the idea touches it), GitHub, npm, and the web in parallel, then synthesizes a decision-ready brief — not code or designs.
---

# Octocode Brainstorming — Idea Research & Exploration

Explore an idea space and turn a raw idea into an evidence-grounded brief. This is **exploratory research**: clarify the idea, map what exists, find the gaps, and pressure-test the opportunity while keeping only the context that helps the next decision. Output is a decision-ready brief — never designs, specs, or code. For build-ready ideas or "how do I build it", prepare a handoff to `octocode/octocode-rfc-generator`; this skill stops at "is it worth building, and where's the white space." Read `references/octocode.md` when choosing, installing, or explaining Octocode MCP vs CLI usage.

```text
FRAME → DIVERGE → RESEARCH (parallel) → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE
```

## Diverge before you converge

Two modes; **mixing them kills both**. Run divergence *first* and *visibly*, then converge hard with evidence.

- **Diverge** — expand framings/options. Defer all judgment; quantity first; combine and build. No "won't work" yet.
- **Converge** — research prior art, run the three-perspective evidence challenge, weigh evidence, decide.

The first framing the user typed is rarely the best one to research — locking onto it anchors every search. Never critique while generating; never generate while deciding.

**Mode scales divergence to the ask.** State it in one line before diverging; default **Validate** when ambiguous.

| User asks | Mode | Diverge | Converge |
|-----------|------|---------|----------|
| "brainstorm ideas for X", "what could I build in Y" | **Generate** | Heavy — 6–10 angles, then validate the top 2–3 | Validate the shortlist |
| "validate my idea", "is X worth building" | **Validate** | Light — 2–4 reframings so research isn't anchored | Heavy — full research + perspective review |
| "has anyone built X", "prior-art options for Y" | **Map** | Minimal — adjacent search terms only | Research-led landscape map |

## Operating principles

- **Assume nothing is novel** — find who tried it, where they stopped, and why.
- **Follow the trail** — README → blog → competitor → issues → the hard unsolved problem.
- **Cross-pollinate** — web names a tool → search its repo/pkg; a repo links docs → read them; a complaint about lib X → verify in code. Each surface sharpens the other's queries.
- **Go deep when thin** — read code, issues, PRs, download trends. Shallow matches are starting points, not answers.
- **Synthesize, don't summarize** — original analysis of what the landscape means, not a link list.

## Hard Gates

Stop and ask before passing any. State the situation in 1–2 lines, name options, recommend one. Never continue silently; never ask outside a gate.

1. **Idea too broad** — maps to 3+ unrelated problem spaces. Usually shows in Frame & Diverge when the slate fans into disconnected domains. Stop before research; ask the user to pick a framing or confirm a shallow sweep.
2. **Zero results** — after research, every active surface returned <2 meaningful hits each, even post synonym-expansion. Don't run perspective review; present what you have, flag the gap, ask: narrow / broaden / accept thin evidence.
3. **Contradictory evidence** — crowded on one surface, "unsolved" on another. Don't bury it; surface both sides with citations and ask which signal to weight.
4. **Worker ceiling** — max **5 delegated workers** per session. Clarification uses 0 workers; the perspective review uses up to 3 workers total (Critical Architect, Visionary Entrepreneur, Product) plus optional targeted cross-exam only if budget remains. Do not run a separate two-role debate plus the panel. No delegation tool or budget left → run labeled sequential lenses and note `perspective review shortened (budget)`.

## Tools

Research surfaces. **Before searching, declare a Surface Plan**: active/skipped + reason for Local, GitHub/packages, and Web. Then **read `references/tools.md`** for exact commands, flags, and per-surface query craft before/while running step 4:
- **GitHub & packages** — Octocode CLI (`npx octocode search --target repositories|packages|pullRequests|commits`, remote structure/content/search, and raw tools for schema-exact/bulk).
- **Local workspace** — unified `search`: `--tree`, text/regex/file discovery, `--pattern`/`--rule`, `--content-view`, and `--op`. Orient here **first** when the idea targets the user's own repo; skip for purely external ideas.
- **Web** — Tavily/Serper/runtime web search are discovery only; fetch/open final evidence before citing. Prefer formal sources: official docs/specs, protocol RFCs, academic papers/indexes, language/framework references, and canonical awesome-lists.

## Workflow

Clarify → Frame & Diverge → Hypothesis map → Parallel research → Cross-pollinate → Perspective Review → Synthesize → Reflect → Present.

**1. Clarify** — one focused question only when audience, problem space, or success criterion is missing; otherwise restate assumptions and continue. Do not delegate clarification. For substantial, multi-turn, or subagent-heavy runs, read `references/hook-communication.md` before research.

**2. Frame & Diverge** (defer judgment) — before any tool, expand the idea space with the lenses below; use a "How might we..." framing for Generate mode. Capture every output, don't filter. Volume by mode (Generate 6–10, Validate 2–4, Map: search terms only).

| Lens | Ask of the idea |
|------|-----------------|
| **Reframe** | What problem is this *really* solving? State it 2–3 ways. |
| **Invert** | What would guarantee it fails / is unnecessary? (→ real risks and moats) |
| **Analogize** | Who solved a structurally similar problem in another domain? |
| **Decompose** | First principles: irreducible parts — which is the hard/novel one? |
| **Combine/shift** | SCAMPER: Substitute, Combine, Adapt, Modify, Put-to-other-use, Eliminate, Reverse. |

Output a compact **framing slate**, then converge once: pick 1–3 framings to research and say why. Feed the reframings/analogies into search expansion.

**3. Hypothesis map** — per chosen framing, 4 bullets: **Crowded if / Underserved if / Blocked if / Worth prototyping if**. A plan, not a conclusion; revise as evidence lands.

**4. Parallel research** — execute the Surface Plan (see `references/tools.md`): GitHub + packages (CLI, main agent), and web products / community / adjacent angles (workers, or main agent if no delegation). Keep a claim ledger (`claim -> source -> confidence -> next query`) and carry only useful claims forward, not raw dumps. For web, start with formal sources and cite fetched/opened pages or papers, never search snippets.
- **Local first (conditional):** if the idea targets the user's own repo, run the Local orient flow *before* external surfaces — establish what exists and the real stack, then frame every GitHub/npm/web query with it. Skip for purely external ideas.
- **Cross-pollinate:** web tool name → `search --target repositories` + `search --target packages`; repo link → read it; package README competitors → search both surfaces; web "unsolved" claim → `search`/`ghSearchCode` to see if anyone solved it in code.
- **CHECKPOINT — before perspective review:** (1) ≥1 cross-pollination query per surface, received and incorporated; (2) any zero-result surface got ≥1 synonym-expanded retry before being marked failed. Skip cross-pollination only if the worker-ceiling gate fired (note "cross-pollination skipped (budget)").
- **Stop when** one more generic search won't change the verdict, every major claim has a source or `weak` marker, and contradictions are gated. **One more pass when** the weakest major claim lacks a source, both sides lean on the same unverified assumption, or one surface strongly contradicts the others without tripping Gate 3.

**5. Perspective Review** (converge) — run the structured evidence challenge with Critical Architect, Visionary Entrepreneur, and Product lenses, then referee what survived. **Follow `references/debate.md`** for role prompts, citation rules, cross-exam, and budget.

**6. Synthesize** — analyze, don't list. Build the verdict from claims that **survived review**, not raw persona output. Agreement → high-confidence, lead with it. Disagree (still contested) → decision point, with evidence. Uncountered risk → blocker; unchallenged strength → best direction. Every claim needs a source.

**7. Reflect** (privately) — weakest claim, best contradiction, decision delta, the one cheap search that could flip the verdict, and **whether a set-aside framing now looks stronger**. Act on it if cheap and ungated; else note why in the TL;DR.

**8. Present** — chat first; scale sections to real content. **Use `references/output.md`** for the compact chat skeleton, confidence markers, RFC handoff packet, and evidence rules. On a confirmed save, write the fuller brief with `references/brief-template.md`. When changing this skill, run `scripts/eval-brainstorm.mjs --self-test` and smoke it with `evals/prompts.md` / `evals/cases.json`.

## Error recovery

| Situation | Action |
|-----------|--------|
| Octocode CLI / native addon fails | Try system Node path; else continue web-only, flag in TL;DR |
| GitHub rate-limited | Reduce concurrency; continue |
| Search key missing/invalid | Try the other engine → fallback chain; give absolute `.env` path |
| All web tools down | GitHub-only; flag in TL;DR |

Broad / zero-result / contradictory ideas are handled by **Hard Gates 1–3** — stop and ask there. To justify or trace a method/tooling claim (diverge-then-converge, defer-judgment, SCAMPER, web-engine API contracts), read `references/grounding.md` when challenged.
