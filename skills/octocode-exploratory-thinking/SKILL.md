---
name: octocode-exploratory-thinking
description: "Use when a user asks the agent to apply a creative reasoning mode, think differently, or use an edge prompt while completing any task. substances are used as a methaphor for traits"
---

# Octocode Exploratory Thinking
tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-brainstorming`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; this skill needs none.

These substance names are **metaphors for reasoning moves**, not pharmacology or advice to use drugs. Apply this lens to the user's current task without changing its permissions or execution rules.

Flow: `TASK → MODE → EDGE → CHECK → DELIVER`.

1. **Task:** Identify the requested result, its constraints, and the obvious next move. Keep doing the user's task; this skill changes how the agent approaches it.
2. **Mode:** Use the user's named mode. Otherwise, for a request to *see differently*, start with Psilocybin, Ketamine, LSD, or Alcohol; use a pace or caution mode as a countercheck. For a request to slow down, focus, coordinate, or persist, pick that control directly.
3. **Edge:** Try one concrete deviation from the obvious move: invert an assumption, test a boundary case, borrow a structural analogy, switch viewpoint or scale, or inspect the no-change case. Apply it to the work itself: a different query, design, edit, test, explanation, or sequence of actions. If it yields only different wording, try one other probe and move on.
4. **Check:** Compare the deviation with the task's success condition and the mode's guardrail. Seek evidence when the task needs it; for research, search terms that could support or challenge the new angle. Stop when another search or probe is unlikely to improve the result.
5. **Deliver:** Complete the requested work in its normal form. Use the edge result if it improves the outcome; otherwise use the stronger ordinary approach.

Each mode adjusts one control of agent behavior; the names are mnemonic labels, not factual claims about substances.

| Mode | Control and move | Check |
|---|---|---|
| Psilocybin / mushrooms | Conceptual distance ↑: import a structural analogy from another domain | Map where the analogy breaks |
| Cannabis / weed | Pace ↓: reread context and notice neglected associations | Return to one concrete step |
| Methamphetamine | Persistence ↑: test one promising lead through short cycles | Set a stop condition; detect loops |
| Caffeine | Attention ↑: isolate the immediate bottleneck | Recheck skipped constraints |
| Alcohol | Inhibition ↓: admit one relevant but initially rejected option | Verify before execution |
| Nicotine | Cycle length ↓: alternate brief hypotheses and checks | Break repetitive patterns |
| Cocaine | Decisiveness ↑: timebox the highest-value probe | Match confidence to evidence |
| Ketamine | Viewpoint distance ↑: change actor, scale, or unit of analysis | Reconcile with observed constraints |
| Sedative | Intervention threshold ↑: examine observation or no-action first | Name evidence that warrants action |
| MDMA | Cooperative modeling ↑: map affected parties' goals and knowledge | Verify intent; never infer consent or authority |
| LSD / psychedelic | Category flexibility ↑: recombine two apparently separate problem frames | Test the new frame against reality |
| Opioid | Noise tolerance ↑: separate transient failures from a trend | Inspect critical warnings explicitly |

Return the result the user asked for. Mention the mode or edge probe only when requested or when it explains a consequential choice. Create no separate report or artifact unless requested; use other skills only when the task itself calls for them.
