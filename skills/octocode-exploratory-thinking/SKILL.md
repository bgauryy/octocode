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
2. **Mode:** Use the user's named mode. Otherwise pick the row whose "Use when" matches the task and whose "Skip when" does not; for a request to *see differently*, start with Psilocybin, Ketamine, LSD, or Alcohol; for auditing or roasting a tool or output, start with Ketamine; use a pace or caution mode as a countercheck. For a request to slow down, focus, coordinate, or persist, pick that control directly.
3. **Edge:** Try one concrete deviation from the obvious move: invert an assumption, test a boundary case, borrow a structural analogy, switch viewpoint or scale, or inspect the no-change case. Apply it to the work itself: a different query, design, edit, test, explanation, or sequence of actions. If it yields only different wording, try one other probe and move on.
4. **Check:** Compare the deviation with the task's success condition and the mode's guardrail. Seek evidence when the task needs it; for research, search terms that could support or challenge the new angle. Stop when another search or probe is unlikely to improve the result.
5. **Deliver:** Complete the requested work in its normal form. Use the edge result if it improves the outcome; otherwise use the stronger ordinary approach.

Each mode adjusts one control of agent behavior. The Brain TL;DR column is a simplified summary of each substance's main neurochemical effect, included so the metaphor carries its meaning; it is not medical guidance, and the agent move is an analogy, not a claim that the substance improves thinking.

Pick by fit, not novelty. "Use when" and "Skip when" come from real octocode flows (tool audits, debugging, perf, releases, reviews).

| Mode | Brain TL;DR (chemistry → effect) | Control and move | Use when | Skip when | Check |
|---|---|---|---|---|---|
| Psilocybin / mushrooms | Psilocin activates serotonin 5-HT2A receptors; the default mode network loosens and distant brain regions talk more → unusual associations | Conceptual distance ↑: import a structural analogy from another domain | Design is stuck and another field solved the same shape (screening candidates like triage, ranking like IR) | Exact lookups, known-cause bugs, contract fixes—bytes decide, not analogies | Map where the analogy breaks |
| Cannabis / weed | THC activates CB1 cannabinoid receptors (hippocampus, prefrontal) → slower time sense, drifting associations, weaker short-term memory | Pace ↓: reread context and notice neglected associations | Long session drifted, or you circle the same files; reread the request, prior results, memory | The bottleneck is already clear, or it's a hot fix under a timebox | Return to one concrete step |
| Methamphetamine | Floods dopamine and norepinephrine (reverses their transporters) → intense drive and focus, prone to perseveration and repetitive loops | Persistence ↑: test one promising lead through short cycles | One reproducible lead needs tracing end to end (bad output → runtime function → removed helper → vestigial test) | Many unranked leads, or checks hit rate-limited or flaky external state—loops burn budget | Set a stop condition; detect loops |
| Caffeine | Blocks adenosine A1/A2A receptors (the fatigue signal) → alertness and narrower focus | Attention ↑: isolate the immediate bottleneck | Perf/build-speed work or one failing test with a measurable bottleneck (crate built 3× under feature sets) | Broad audits or ideation where the bottleneck is unknown—it tunnels past the real issue | Recheck skipped constraints |
| Alcohol | Boosts GABA-A inhibition and blocks NMDA glutamate; prefrontal control drops first → disinhibition, weaker judgment | Inhibition ↓: admit one relevant but initially rejected option | An option was dismissed by habit, not evidence (a crate you "never use", a flag you "can't change") | Security, secrets, destructive or publish actions—never admit a rejected unsafe option | Verify before execution |
| Nicotine | Activates nicotinic acetylcholine receptors (α4β2), releasing dopamine; short half-life → brief attention boosts in quick cycles | Cycle length ↓: alternate brief hypotheses and checks | Checks are cheap (sub-second CLI call, single unit test, one search) | Each check is expensive (release build, provider-billed classification, live API)—batch instead | Break repetitive patterns |
| Cocaine | Blocks dopamine (also norepinephrine and serotonin) reuptake; short-acting → confidence and decisiveness, risk of grandiosity | Decisiveness ↑: timebox the highest-value probe | Many findings compete and the user wants a ranked verdict or one next action | Evidence is thin—decisiveness becomes inflated confidence (hardcoded "high") | Match confidence to evidence |
| Ketamine | Blocks NMDA glutamate receptors → dissociation: a detached, outside-observer view of self and scene | Viewpoint distance ↑: change actor, scale, or unit of analysis | Evaluating a tool, API, output, or doc: become its consumer (the agent reading output) to catch results that look like evidence but mislead | Internal refactors with no consumer, or exact fact lookups | Reconcile with observed constraints |
| Sedative | Benzodiazepines/barbiturates strengthen GABA-A inhibition → lower arousal and anxiety, slower reaction, higher threshold to act | Intervention threshold ↑: examine observation or no-action first | A reported bug may be stale or misread (external review, old backlog, "already fixed?") | A confirmed repro of security, data loss, or a contract break—act | Name evidence that warrants action |
| MDMA | Releases serotonin (plus oxytocin, norepinephrine, dopamine) → empathy, trust, sensing others' perspectives | Cooperative modeling ↑: map affected parties' goals and knowledge | Cross-package or public contract changes (core ↔ native ↔ MCP ↔ CLI ↔ skills), releases, concurrent agents on one tree | Single-owner local edits with one consumer | Verify intent; never infer consent or authority |
| LSD / psychedelic | Activates 5-HT2A (plus dopamine D2) for hours; perceptual category boundaries blur → recombining unrelated frames | Category flexibility ↑: recombine two apparently separate problem frames | Two symptoms may share one root (schema bloat + mandatory fields; duplicate extractors drifting apart) | Frames are truly separate and a small fix is due—recombining widens scope | Test the new frame against reality |
| Opioid | Activates μ-opioid receptors → dampens pain and distress signals, lowers reactivity; danger is ignoring real alarms (respiratory depression) | Noise tolerance ↑: separate transient failures from a trend | Flaky CI, benchmark variance, test failures from a concurrent session's edits | Security warnings, secret leaks, contract violations, data loss—inspect each one | Inspect critical warnings explicitly |

Return the result the user asked for. Mention the mode or edge probe only when requested or when it explains a consequential choice. Create no separate report or artifact unless requested; use other skills only when the task itself calls for them.
