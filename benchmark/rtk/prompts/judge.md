# Judge agent prompt

Paste this whole file to the agent. The operator replaces the two `<RUN_*>` placeholders with absolute paths to the two completed runs before pasting.

---

```
RUN_OCTO: <RUN_OCTO_PATH>   # ← operator: absolute path to the octocode run dir
RUN_RTK:  <RUN_RTK_PATH>    # ← operator: absolute path to the rtk run dir

ROLE
You are an evaluation agent. Two research agents (octocode + rtk) have already
answered the questions in QUESTIONS.md about the rtk-ai/rtk repository. Your
job: read both runs, judge answer quality by independently fact-checking each
answer against the live rtk-ai/rtk repository, compare quality-adjusted token
usage, and write a comparison summary.

IMPORTANT: this benchmark's metering is character-based (in_chars + out_chars)
because it is tokenizer-independent and deterministic. Treat characters as the
canonical token-usage proxy. Wall-clock time is context only, not the winner axis.

Blind runs use judge verification instead of an EXPECTED_FACTS.md answer key. Establish the facts
from the rtk-ai/rtk GitHub repository and source code. Judge
research/tool calls are outside the measured researcher runs.

INPUTS TO READ
1. benchmark/rtk/QUESTIONS.md                           → what was asked
2. $RUN_OCTO/output.md + $RUN_OCTO/summary.json         → octocode rollup
3. $RUN_RTK/output.md  + $RUN_RTK/summary.json          → rtk rollup
4. Per Q (n = 1..N):
     $RUN_OCTO/q<n>.md     → octocode's answer + metadata
     $RUN_OCTO/q<n>.json   → octocode's per-Q numbers
     $RUN_RTK/q<n>.md      → rtk's answer + metadata
     $RUN_RTK/q<n>.json    → rtk's per-Q numbers

FACT-CHECKING REQUIREMENT
For every question, independently verify the load-bearing facts before scoring.
Use the live rtk-ai/rtk repository: source files, PR bodies, PR comments,
reviews, CHANGELOG, docs. Treat both agents as hypotheses until verified.

═══════════════════════════════════════════════════════════════════
STEP 1 — Semantic per-Q evaluation
═══════════════════════════════════════════════════════════════════

For each Q, read the question, verify the load-bearing facts independently,
then read both q<n>.md files. Score each agent:

  A. ANSWER QUALITY (0–3):
       3 — every load-bearing fact present, no false claims, all sub-questions
           answered, exact comment text quoted where required
       2 — mostly correct, one load-bearing sub-fact missing or inaccurate
       1 — partially correct, OR unsupported claim present
       0 — unsupported, empty, or UNKNOWN

     CAPABILITY SCORING NOTES (apply these on top of the standard rubric):

     COMMENT PRESERVATION (Q3, Q4, Q5, Q19):
       If an answer is missing information that exists only in source comments
       (e.g., TODO text, doc comment intent, SAFETY annotations), score according
       to how much of the requested fact pattern remains supported. Note the
       specific comment text that was missed.

     RESULT COMPLETENESS (Q1, Q2, Q15):
       If an answer provides an incomplete count and the agent noted a tool cap
       (e.g., "rtk rg caps at 200 results"), score the partial answer according
       to the supported facts and note the disclosed limit.
       If the agent provides a count without disclosing it may be incomplete,
       verify independently; if it is incomplete and undisclosed, score as 1.

     PR METADATA COVERAGE (Q10, Q11, Q12, Q17):
       Missing labels, missing PR comment content, or missing PR discussion
       points are treated as missing load-bearing facts and score at most 2.
       If ALL three of labels, comments, and file-change list are absent, score 1.

     REMOTE CONTENT BREADTH (Q13, Q14, Q20):
       If an answer is visibly incomplete and the rtk researcher was capped at
       2000 chars, note the cap and score proportionally to what was retrieved.

  B. TOKEN / CHARACTER USAGE (from q<n>.json + summary.json):
       amortized_mcp_init_chars =
         octocode: (mcp_init.in_chars + mcp_init.out_chars) / N_answered
         rtk:      0
       effective_chars = in_chars + out_chars + amortized_mcp_init_chars
       token_score     = quality / (effective_chars / 1000)

  C. QUALITY-ADJUSTED TOKEN SCORE:
       Higher is better. A cheaper unsupported answer is not a win.

  D. HONESTY:
       Did the agent write UNKNOWN when blocked by a tool limitation, or did
       it guess? For rtk specifically: did it correctly report that a capability
       was out of scope (npm registry, remote directory, PR comments) rather
       than inventing an answer?

  PER-Q WINNER (non-drift only):
     A wins iff token_score(A) > token_score(B). Within 5%: tie.

═══════════════════════════════════════════════════════════════════
STEP 2 — Write to benchmark/rtk/output/summary.md
═══════════════════════════════════════════════════════════════════

Requested sections, in order:

  # Benchmark summary — octocode vs rtk

  3–5 sentence intro: which agents ran, what the questions test, and
  the headline winner on quality-adjusted token usage.

  ## Per-question table

  | Q | Category | Drift | Octo qual | rtk qual | Octo chars | rtk chars | Octo token score | rtk token score | Winner | Notes |

  - Category: one of: Code Search · File Content · Directory · File Metadata ·
    PR Research · GitHub Content · Cross-cutting
  - Quality: your semantic score (0–3). Prefix drift Q scores with "d:".
  - Notes: cite the specific capability difference when rtk scores lower
    (e.g., "comment text missing", "result limit disclosed", "PR labels absent",
    "remote content truncated", "package registry out of scope").

  ## Quality verdict (non-drift Qs only)

  | Agent | Σ quality | Token-score wins | Token-score ties | Avg quality per Q |

  2–3 sentences: which question categories each agent answered more
  accurately, and whether either agent's token efficiency came at a
  quality cost.

  ## Capability Review

  For each Q where rtk scored lower than octocode, state:
  - Q number and question category
  - The specific capability difference (comment preservation / result completeness /
    PR metadata coverage / remote content breadth / out-of-scope capability)
  - The exact information that was unreachable via rtk

  ## Quality-adjusted token-usage verdict

  | Axis | octocode | rtk | ratio (octo/rtk) |
  | Σ quality (non-drift)          |   |   | |
  | Σ calls                        |   |   | |
  | Σ in_chars (per-Q)             |   |   | |
  | Σ out_chars (per-Q)            |   |   | |
  | MCP init chars                 |   | 0 | |
  | TOTAL chars (per-Q + init)     |   |   | |
  | Approx tokens (TOTAL/4)        |   |   | |
  | Quality per 1k chars           |   |   | |
  | Σ tool_elapsed_ms (context)    |   |   | |
  | Σ q_elapsed_ms (context)       |   |   | |

	  3–4 sentences interpreting the table. Is rtk's token saving
	  worth the capability tradeoff? Which categories does each agent dominate?

  ## Run-Quality Review

  Bullets:
  - Unsupported facts (Q, agent, what was inaccurate).
  - Qs where rtk correctly wrote UNKNOWN vs qs where it guessed.
  - Token-consumption pathologies (unfiltered full-file dumps, etc.).
  - Questions where both agents performed poorly.

  ## Verdict

  ≤4 sentences. State the quality-adjusted token-usage winner. State
  raw-quality winner separately. Call out the capability tradeoff explicitly:
  what research capability does rtk sacrifice for token savings?

═══════════════════════════════════════════════════════════════════
VALIDITY CHECKLIST
═══════════════════════════════════════════════════════════════════

• Independently verify facts before scoring. Treat each agent answer as a hypothesis until verified.
• Cite specific file paths, comment text, PR discussion points for every score below 3.
• Include MCP init chars in octocode totals.
• Use quality per measured character as the winner axis; wall-clock time is context only.
• Output one file: benchmark/rtk/output/summary.md.
```
