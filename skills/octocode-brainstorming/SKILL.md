---
name: octocode-brainstorming
description: Use when the user wants to brainstorm or validate an idea grounded in evidence — triggers like "brainstorm", "is this worth building", "has anyone built X", "validate my idea", "check if X exists", "research this idea", "prior-art options for Y", "should we add X to our app/codebase". Researches the local workspace (when the idea touches it), GitHub, npm, and the web in parallel, then synthesizes a decision-ready brief — not code or designs.
---

# Octocode Brainstorming — Idea Research & Exploration

Explore an idea space and turn a raw idea into an evidence-grounded brief. This is **exploratory research**: map what exists, find the gaps, and pressure-test the idea — across the local workspace (when relevant), GitHub, npm, and the web. Output is a decision-ready brief — never designs, specs, or code. For "how do I build it" hand off to `octocode-rfc-generator`; this skill stops at "is it worth building, and where's the white space."

```text
FRAME → DIVERGE → RESEARCH (parallel) → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE
```

## Diverge before you converge

Two modes; **mixing them kills both**. Run divergence *first* and *visibly*, then converge hard with evidence.

- **Diverge** — expand framings/options. Defer all judgment; quantity first; combine and build. No "won't work" yet.
- **Converge** — research prior art, stress-test (Advocate vs Critic), weigh evidence, decide.

The first framing the user typed is rarely the best one to research — locking onto it anchors every search. Never critique while generating; never generate while deciding.

**Mode scales divergence to the ask.** State it in one line before diverging; default **Validate** when ambiguous.

| User asks | Mode | Diverge | Converge |
|-----------|------|---------|----------|
| "brainstorm ideas for X", "what could I build in Y" | **Generate** | Heavy — 6–10 angles, then validate the top 2–3 | Validate the shortlist |
| "validate my idea", "is X worth building" | **Validate** | Light — 2–4 reframings so research isn't anchored | Heavy — full research + Advocate/Critic |
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
2. **Zero results** — after research, all three surfaces returned <2 meaningful hits each, even post synonym-expansion. Don't run Advocate/Critic; present what you have, flag the gap, ask: narrow / broaden / accept thin evidence.
3. **Contradictory evidence** — crowded on one surface, "unsolved" on another. Don't bury it; surface both sides with citations and ask which signal to weight.
4. **Worker ceiling** — max **5 delegated workers** per session (web slices + the Advocate/Critic debate, up to 4 dispatches across two rounds). If more seem needed, synthesize first and ask for a second pass. No delegation tool → same 5-slot budget, run the debate as sequential labeled passes.

## Tools

Three research surfaces. **Read `references/tools.md`** for the exact commands, flags, and per-surface query craft before/while running step 4:
- **GitHub & packages** — Octocode CLI (`search --target repositories|packages|commits`, remote structure/content/search, `pr`, and raw tools for schema-exact/bulk).
- **Local workspace** — unified `search`: `--tree`, text/regex/file discovery, `--pattern`/`--rule`, `--content-view`, and `--op`. Orient here **first** when the idea targets the user's own repo; skip for purely external ideas.
- **Web** — `scripts/serper-search.mjs` / `scripts/tavily-search.mjs` (`--check` keys at startup), then read + follow leads with the runtime web reader; fallback to README/awesome-list/aggregator seeds when no key. **When searching a subject, feature, or library: prefer formal sources first** — official docs, IETF/W3C/ISO specs, protocol RFCs, language/framework reference docs, and canonical awesome-lists — before blog posts or secondary aggregators.

## Workflow

Clarify → Frame & Diverge → Hypothesis map → Parallel research → Cross-pollinate → Advocate vs Critic → Synthesize → Reflect → Present.

**1. Clarify** — one focused question only if ambiguous; else skip.

**2. Frame & Diverge** (defer judgment) — before any tool, expand the idea space with the lenses below. Capture every output, don't filter. Volume by mode (Generate 6–10, Validate 2–4, Map: search terms only).

| Lens | Ask of the idea |
|------|-----------------|
| **Reframe** | What problem is this *really* solving? State it 2–3 ways. |
| **Invert** | What would guarantee it fails / is unnecessary? (→ real risks and moats) |
| **Analogize** | Who solved a structurally similar problem in another domain? |
| **Decompose** | First principles: irreducible parts — which is the hard/novel one? |
| **Combine/shift** | SCAMPER: Substitute, Combine, Adapt, Modify, Put-to-other-use, Eliminate, Reverse. |

Output a compact **framing slate**, then converge once: pick 1–3 framings to research and say why. Feed the reframings/analogies into search expansion.

**3. Hypothesis map** — per chosen framing, 4 bullets: **Crowded if / Underserved if / Blocked if / Worth prototyping if**. A plan, not a conclusion; revise as evidence lands.

**4. Parallel research** — hit **all three surfaces** (see `references/tools.md`): GitHub + packages (CLI, main agent), and web products / community / adjacent angles (workers, or main agent if no delegation). For web, **start with authoritative sources**: official docs, IETF/W3C/ISO specs, protocol RFCs, and framework references — they define ground truth; blogs and tutorials come after.
- **Local first (conditional):** if the idea targets the user's own repo, run the Local orient flow *before* external surfaces — establish what exists and the real stack, then frame every GitHub/npm/web query with it. Skip for purely external ideas.
- **Cross-pollinate:** web tool name → `search --target repositories` + `search --target packages`; repo link → read it; package README competitors → search both surfaces; web "unsolved" claim → `search`/`ghSearchCode` to see if anyone solved it in code.
- **CHECKPOINT — before Advocate/Critic:** (1) ≥1 cross-pollination query per surface, received and incorporated; (2) any zero-result surface got ≥1 synonym-expanded retry before being marked failed. Skip cross-pollination only if the worker-ceiling gate fired (note "cross-pollination skipped (budget)").
- **Stop when** one more generic search won't change the verdict, every major claim has a source or `weak` marker, and contradictions are gated. **One more pass when** the weakest major claim lacks a source, both sides lean on the same unverified assumption, or one surface strongly contradicts the others without tripping Gate 3.

**5. Advocate vs Critic** (converge) — run the structured two-round debate, then assemble the best-of-both verdict. **Follow `references/debate.md`** for the exact round-1/round-2 prompts, the referee step, and the budget rule.

**6. Synthesize** — analyze, don't list. Build the verdict from claims that **survived rebuttal** (best-of-both), not the raw Round-1 lists. Agree → high-confidence, lead with it. Disagree (still contested) → decision point, both sides with evidence. Uncountered risk → blocker; unchallenged strength → best direction. Every claim needs a source.

**7. Reflect** (privately) — weakest claim, best contradiction, decision delta, the one cheap search that could flip the verdict, and **whether a set-aside framing now looks stronger**. Act on it if cheap and ungated; else note why in the TL;DR.

**8. Present** — chat first; scale sections to real content. **Use `references/output.md`** for the compact chat skeleton, confidence markers, and evidence rules. On a confirmed save, write the fuller brief with `references/brief-template.md`.

## Error recovery

| Situation | Action |
|-----------|--------|
| Octocode CLI / native addon fails | Try system Node path; else continue web-only, flag in TL;DR |
| GitHub rate-limited | Reduce concurrency; continue |
| Search key missing/invalid | Try the other engine → fallback chain; give absolute `.env` path |
| All web tools down | GitHub-only; flag in TL;DR |

Broad / zero-result / contradictory ideas are handled by **Hard Gates 1–3** — stop and ask there. To justify or trace a method/tooling claim (diverge-then-converge, defer-judgment, SCAMPER, web-engine API contracts), read `references/grounding.md` when challenged.
