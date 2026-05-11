# Outline: From Code Search to AI-Powered Research Engine

**Depth:** Technical live talk for practicing engineers. The deck explains Octocode as a daily research companion first, frames the protocol that keeps the research loop honest, then drops into a Wix Engineering case study (Bilbo) showing what those primitives look like at production scale, and closes on daily practice + lessons.

**Arc:** problem → engine → protocol → **case study (Wix · Bilbo)** → daily practice → lessons → close. Every section opens with a question the previous section raised, and closes with the question the next section answers.

---

## Story beats (5 acts, 31 slides + 1 appendix)

| Act | Slides | Question this act answers |
|-----|--------|---------------------------|
| **I · The problem** | 01–04 | What goes wrong when an agent doesn't have real code context? |
| **II · Octocode is a research engine** | 05–11 | What does Octocode do, and what contract makes it predictable? |
| **III · Case study · Wix Engineering (Bilbo)** | 12–23 | What happens when you scale these primitives into a multi-agent investigator? |
| **IV · Lessons from shipping Bilbo** | 24–30 | What did building it actually teach us? |
| **V · Close** | 31 | What's the takeaway? |
| **Appendix** | 32 | (Q&A reference: full tool / skill / prompt surface) |

---

## Visible playlist (31 slides) + appendix

### Act I · The problem (4)
| # | Title (claim) | Type | File |
|---|---|---|---|
| 01 | From Code Search to AI-Powered Research Engine | title | `slides/title.html` |
| 02 | AI agents are only as good as the context they can see | comparison | `slides/ai-agents-need-code-context.html` |
| 03 | Without real code intelligence, agents just guess wrong faster | content | `slides/real-code-intelligence-prevents-guessing.html` |
| 04 | I spend more time finding code than writing it | timeline | `slides/from-github-search-to-research-engine.html` |

### Act II · Octocode is a research engine (7)
| # | Title (claim) | Type | File |
|---|---|---|---|
| 05 | A code research engine for GitHub — built for agents and humans | content | `slides/octocode-mcp.html` |
| 06 | Octocode answers the six questions engineers ask every day | cards | `slides/capabilities-map-to-questions.html` |
| 07 | Code research is a loop — every step asks: do I have enough? | flow | `slides/research-loop.html` |
| 08 | The Octocode protocol enforces a predictable research loop | content | `slides/octocode-research-protocol.html` |
| 09 | Every call carries the question | code | `slides/protocol-request-envelope.html` |
| 10 | Every response guides the next move | code | `slides/protocol-response-hints.html` |
| 11 | Good results turn into the next move (NEXT · VALIDATE · PIVOT) | cards | `slides/results-turn-into-next-move.html` |

### Act III · Case study · Wix Engineering (12)
> Slide 11 ends by setting up the case study. Slide 12 is the explicit gear-shift with a "Case Study · Wix Engineering" eyebrow. Slide 23 closes the section before returning to general practice.

| # | Title (claim) | Type | File |
|---|---|---|---|
| 12 | Meet Bilbo. _(Case Study · Wix Engineering)_ | title | `slides/wix-research-title.html` |
| 13 | A multi-agent AI system that investigates hard system questions end to end | content | `slides/wix-research-is-multi-agent-investigation.html` |
| 14 | Investigations must scale across the entire organization at once | content | `slides/wix-research-investigations-cross-boundaries.html` |
| 15 | Typed handoffs separate planning from execution | content | `slides/planner-researcher-contract.html` |
| 16 | The Planner emits a strict research contract | content | `slides/wix-research-planner-brief.html` |
| 17 | The Researcher follows evidence and delegates depth | content | `slides/wix-researcher-follows-evidence.html` |
| 18 | Helpers absorb the deep-read cost | content | `slides/wix-research-helpers-stay-isolated.html` |
| 20 | Tool scope is part of the reasoning contract | code | `slides/tool-scope-reasoning-contract.html` |
| 20b | Dynamic helpers assemble their scope at call time | code | `slides/helper-built-per-call.html` |
| 21 | Delegate the heavy work — keep the rest in-context | cards | `slides/helper-isolation-prevents-context-fatigue.html` |
| 22 | Each investigation produces durable knowledge | content | `slides/wix-research-memory-starts-ahead.html` |
| 23 | Shipping Bilbo taught us that context engineering is the real work _(Act V opener)_ | closing | `slides/wix-research-grounded-answer.html` |

### Act IV · Lessons from shipping Bilbo (7)
| # | Title (claim) | Type | File |
|---|---|---|---|
| 24 | Vendor CLIs were a ceiling, so we built our own agent | comparison | `slides/coding-agent-build-vs-buy.html` |
| 25 | We picked Google ADK — then forked it to ship faster | content | `slides/framework-fork-google-adk.html` |
| 26 | Tools as MCPs — agents and engineers share one surface | diagram | `slides/tools-as-mcps.html` |
| 27 | Context is the brain, and it requires hand-curation | cards | `slides/missing-piece-context.html` |
| 28 | Some decisions still need a human — AI can't weigh real trade-offs | comparison | `slides/human-in-the-loop-decisions.html` |
| 29 | Engineering context means tuning prompts, budgets, and schemas | cards | `slides/context-engineering-lessons.html` |
| 30 | Quality requires continuous measurement and reflection loops | content | `slides/evals-quality-kpis.html` |

### Act V · Close (1)
| # | Title (claim) | Type | File |
|---|---|---|---|
| 31 | Better context makes better agents | closing | `slides/better-context-makes-better-agents.html` |

### Appendix (1, after closing — for Q&A reference)
| # | Title | Type | File |
|---|---|---|---|
| 32 | Octocode ships 14 tools, 17 skills, 7 prompts — one harness | cards | `slides/octocode-harness.html` |

---

## Hidden alternates (kept on disk for swap-in or future cuts)

- `evidence-behavior-evals` — earlier daily-practice slide on what good evals look like; replaced by the dedicated `evals-quality-kpis` lesson slide that ties evals to reflection loops.
- `challenges-and-learnings` — original 4-lesson grid; superseded by the 7 focused lesson slides (build vs vendor, framework fork, MCPs, context, human-in-the-loop, context engineering, evals).
- `ai-agents-need-context` — 8-card alternate problem-frame (replaced by tighter 4-bullet `ai-agents-need-code-context`).
- `code-research-loop`, `daily-research-companion`, `six-code-intelligence-moves`, `research-branches-on-evidence` — earlier loop variants subsumed by `research-loop` and the protocol slides.
- `wix-research-one-loop-many-roles`, `wix-research-skills-encode-playbooks`, `wix-research-same-tools-for-humans-ai`, `wix-research-three-things-to-remember`, `wix-research-context-stays-sharp` — Wix-section colour added in earlier draft; kept as alternates for a longer slot.
- `ai-powered-research-engine`, `cited-code-map-output`, `interface-serves-agents-and-humans` — early Octocode high-level alternates, now superseded by the Act II protocol arc.
- `helpers-return-typed-findings` — alternate visualization of the same point as `wix-research-helpers-stay-isolated` + `helper-isolation-prevents-context-fatigue`.
- `portable-research-engine`, `typed-findings-at-agent-boundaries`, `context-compaction-correctness` — engine-design tangents now covered (or covered better) inside the Wix Engineering case study.
- `inspectable-research-trail`, `follow-ups-continue-context` — daily-use alternates compressed into Act IV.

---

## Bidirectional validation

- **Top-down (goal → arc → slides):** problem (why agents need code context) → engine (Octocode + protocol) → case study (Bilbo at scale) → lessons → close. Each act owns one question.
- **Bottom-up (titles read as a paragraph):** every title is a claim sentence with a verb. The closing claim ("Better context makes better agents") traces back to slide 02 ("AI agents are only as good as the context they can see") and is reinforced by the Act IV lesson titles (especially slides 27 "Context is the brain" and 29 "Context engineering is the work").
- **Question→Answer chain:** slide 11 (NEXT/VALIDATE/PIVOT) bridges to the case study; slide 23 ("Now — what shipping Bilbo taught us") is the explicit hand-off from architecture to lessons; the seven lesson slides (24–30) each name a specific decision, building toward slide 31's takeaway.

## Density check

- No visible slide carries more than 4 first-level bullets/cards. The dense `ai-agents-need-context` (8 cards) and original 4-card `challenges-and-learnings` are hidden in favor of focused per-lesson slides; `octocode-harness` is in the labelled appendix where reference density is appropriate.
- Layout variety across the 7 new lesson slides: comparison, content, diagram, cards, comparison, cards, content — no 3 consecutive identical types within Act IV.
