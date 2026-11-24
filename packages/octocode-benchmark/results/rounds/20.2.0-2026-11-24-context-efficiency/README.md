# v20 published surface before and after the context-efficiency round

The smaller tool definitions cut input tokens per answer by 10% (95% CI 5–15%) on 69 questions, with no
change in tool calls or output tokens. Answer correctness was not judged in this round.

## Setup

| | |
|---|---|
| Question | Does the published surface of the [context-efficiency RFC](../../../../../docs/rfcs/context-efficiency.md) cost less per answer than the one before it? |
| Arms | **base**: the `tools/list` and instructions served before the round; **landed**: after P1, P2, P3, P4, P8 |
| Binary | one frozen 20.2.0 build for both arms (SHA-256 `6ec8617b…3474`, `20.2.0+54a1f44b`, dirty tree on `ae3a84fc6`); each arm's surface is served by a pass-through proxy, so only the definitions differ |
| Not measured | the native changes (P4 anchoring by name, P5 declaring-hit index, P6 glob hint): both arms ran on the same binary |
| Questions | [GitHub 30](../../../questions/github/README.md) and [local goals 40](../../../questions/local-goals/README.md) |
| Agent | Claude Sonnet 5.5 in Claude Code, fresh session per run, Octocode MCP tools only, no project files or memory |
| Passes | local 3 per arm (240 runs); GitHub 1 per arm, 3 of 30 with 2 (63 runs): the round was stopped early on request, after 14 runs failed on an account usage limit |
| Order | arm order alternates per question and pass |
| Tokens | the provider's usage per run: new input (uncached plus cache writes), cached re-reads, output |

## Results

Per question, the median over its passes per arm; the headline is the geometric mean of landed ÷ base over
questions, with a 95% bootstrap interval.

| Measure | All 69 | GitHub 29 | Local 40 |
|---|---:|---:|---:|
| Input tokens (new + cached) | **0.900** (0.848–0.953) | 0.840 (0.773–0.913) | 0.945 (0.874–1.021) |
| Requests | 0.944 (0.893–0.997) | 0.878 (0.812–0.948) | 0.994 (0.926–1.069) |
| Tool calls | 1.012 (0.933–1.101) | 0.981 (0.853–1.147) | 1.036 (0.943–1.134) |
| Output tokens | 0.988 (0.944–1.031) | 0.966 (0.888–1.049) | 1.004 (0.960–1.048) |
| Wall time | 0.945 (0.906–0.986) | 0.937 (0.868–1.014) | 0.951 (0.908–0.995) |
| Prefix per request, median | 23,189 → 22,088 | 23,073 → 21,974 | 23,210 → 22,110 |
| Failed tool calls | 9 of 524 → 10 of 540 | 2 of 151 → 1 of 152 | 7 of 373 → 9 of 388 |
| Model cost | $7.73 → $7.53 | | |

- Input tokens were lower on 54 of 69 questions, higher on 14. Most of the drop is the 1,100-token smaller
  prefix re-read on every request (−4.7% per request).
- The GitHub drop (16%) is larger than the prefix alone and comes with 12% fewer requests, but rests on one
  pass per arm for 26 of its questions: direction only.
- Tool calls and output tokens did not change measurably.

## Not checked

- **Correctness.** No judge compared the two arms' answers. A change that saves tokens but loses evidence would
  not show here.
- **The native changes.** See the RFC's per-proposal checks (real calls and crate tests).

Raw runs, transcripts, per-question medians (`per-question.json`) and both served surfaces are in
`campaigns/20.2.0-2026-11-24-context-efficiency/` (local).
