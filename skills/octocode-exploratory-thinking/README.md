# Octocode Exploratory Thinking 🔞

> **18+ · Research preview · Exploratory only.** This skill is an experiment: the modes are not benchmarked yet, so there is no measured evidence that they improve results. Use it for exploration, not as a default workflow. (The 18+ label is a joke about the substance names; no substances are involved.)

Change how the agent approaches a task by applying one named reasoning mode, then deliver the task's normal result.

Modes use substance names as mnemonics for reasoning moves. Each one adjusts a single control of agent behavior, such as viewpoint, pace, persistence, or how quickly it acts. They are metaphors, not pharmacology or advice to use drugs.

## Use when

- You ask the agent to think differently, try an edge prompt, or use a named mode ("use Ketamine mode").
- A tool, API, output, or doc needs evaluating from its consumer's viewpoint (Ketamine).
- A session is stuck, drifting, looping, or tunnelling, and a different control would help.

## Not for

- Open-ended ideation or feasibility scoping → `octocode-brainstorming`
- Evidence collection for a code claim → `octocode-research`
- Critique delivery and ranking → `octocode-roast` (this skill can shape how a roast looks)

## Modes at a glance

| Mode | Control | Reach for it when |
|---|---|---|
| Psilocybin | Conceptual distance ↑ | Another domain has solved the same shape |
| Cannabis | Pace ↓ | A long session drifted; reread the request and results |
| Methamphetamine | Persistence ↑ | One reproducible lead needs tracing end to end |
| Caffeine | Attention ↑ | A measurable bottleneck exists (perf, one failing test) |
| Alcohol | Inhibition ↓ | An option was dismissed by habit, not evidence |
| Nicotine | Cycle length ↓ | Checks are cheap and fast |
| Cocaine | Decisiveness ↑ | Many findings compete for one ranked verdict |
| Ketamine | Viewpoint distance ↑ | Auditing a tool or output as its consumer |
| Sedative | Action threshold ↑ | A reported bug may be stale or misread |
| MDMA | Cooperative modeling ↑ | Changes cross packages, owners, or concurrent agents |
| LSD | Category flexibility ↑ | Two symptoms may share one root |
| Opioid | Noise tolerance ↑ | Flaky CI or variance hides the real trend |

`SKILL.md` holds the full table: a brain-chemistry summary for each mode, when to skip it, and the guardrail check.

## Commitments

- Keep doing the requested task; the mode changes the approach, not the deliverable.
- Change nothing about permissions or execution rules.
- Try one concrete edge probe; drop it when it only rewords the obvious move.
- Check each mode against its guardrail. Security, secrets, data loss, and contract breaks are never "noise."
- Mention the mode only when asked or when it explains a consequential choice.

## Workflow

```text
TASK → MODE → EDGE → CHECK → DELIVER
```

## Install

```bash
npx -y octocode skill install octocode-exploratory-thinking
```

## Maintainer verification

Run the `octocode-skills` review against this folder.
