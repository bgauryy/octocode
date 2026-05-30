# Judge agent prompt

Paste this whole file to the agent verbatim. The operator must replace the two `<RUN_*>` placeholders on lines 1–2 with absolute paths to the two completed runs before pasting.

---

```
RUN_OCTO: <RUN_OCTO_PATH>   # ← operator: absolute path to the octocode run dir
RUN_GH:   <RUN_GH_PATH>     # ← operator: absolute path to the gh run dir

ROLE
You are an evaluation agent. Two research agents (octocode + gh) have already
answered the questions in QUESTIONS.md. Your job: read both runs, judge answer
quality by independently fact-checking each answer against GitHub source/PR
facts, compare **quality-adjusted token usage**, and write a comparison summary.

IMPORTANT: this benchmark's metering is character-based (`in_chars` +
`out_chars`) because it is tokenizer-independent and deterministic. Treat
characters as the canonical token-usage proxy. Do NOT make wall-clock time the
winner axis. Time may be reported as context only.

There is intentionally no EXPECTED_FACTS.md answer key. You must establish the
facts yourself. Judge research/tool calls are outside the measured researcher
runs and must not be added to either agent's token-usage totals.

INPUTS YOU MUST READ
1. benchmark/github/QUESTIONS.md          → what was asked
2. $RUN_OCTO/output.md + $RUN_OCTO/summary.json    → octocode rollup
3. $RUN_GH/output.md   + $RUN_GH/summary.json      → gh rollup
4. Per Q (n = 1..N):
     $RUN_OCTO/q<n>.md      → octocode's answer + metadata   (flat in run dir)
     $RUN_OCTO/q<n>.json    → octocode's per-Q numbers
     $RUN_GH/q<n>.md        → gh's answer + metadata
     $RUN_GH/q<n>.json      → gh's per-Q numbers

FACT-CHECKING REQUIREMENT
For every question, independently verify the load-bearing facts before scoring.
Use reliable GitHub evidence: repository source files, PR files, PR comments,
reviews, commit history, and package source as needed. Prefer current upstream
state unless the question points to a fixed PR, version, or date. Do not assume
either agent is correct; both can be wrong.

═══════════════════════════════════════════════════════════════════
STEP 1 — Semantic per-Q evaluation. For EVERY Q (n = 1..N):
═══════════════════════════════════════════════════════════════════

For each Q, read the question, do enough independent verification to know the
load-bearing facts, then read both q<n>.md files in full. For each agent, score
three axes:

  A. ANSWER QUALITY (0–3, semantic — your judgment):
       3 — every load-bearing fact present, no false claims, all requested
           repos / trace steps / PR sub-questions answered
       2 — mostly correct, but one load-bearing sub-fact is missing or wrong
       1 — partially correct, OR a hallucinated claim is present
       0 — wrong, empty, or "UNKNOWN"

     Judge against the exact question wording and independently verified facts.
     The core question is: **how accurate and useful is the answer compared with
     the real answer?** Accept equivalent identifiers, moved/renamed files,
     paraphrases, and extra correct context. Penalize missing required facts,
     unsupported claims, contradictions, wrong files/functions, wrong PR status,
     or vague answers that do not actually answer the requested trace/comparison.

     Multi-part questions are any questions that explicitly ask numbered
     sub-questions, multiple repos, a trace, or a comparison. Score each part
     separately, average to one number, and note the per-part breakdown when it
     changes the score.

     For every non-3 score, write a one-line reason quoting the missing or wrong
     fact (e.g. "missed `throwException` setting `ShouldCapture`").

     Drift questions (heading suffix [drift] in QUESTIONS.md, or questions you
     flag as date-sensitive): score loosely — star counts, recent PR lists, and
     branch HEAD facts can change between runs. Accept any answer that's
     directionally correct, but report drift separately.

  B. TOKEN / CHARACTER USAGE (from q<n>.json + summary.json):
       - calls            : tool invocations
       - in_chars         : agent payload sent, counted as Unicode codepoints
       - out_chars        : agent payload received, counted as Unicode codepoints
       - per_q_chars      : in_chars + out_chars
       - tool_elapsed_ms  : Σ tool wall time (context only; NOT winner axis)
       - q_elapsed_ms     : full Q wall clock (context only; NOT winner axis)
       - reasoning_ms     : q_elapsed - tool (context only; NOT winner axis)

     Compute token/character usage for each Q:
       amortized_mcp_init_chars =
         octocode: (mcp_init.in_chars + mcp_init.out_chars) / N_answered
         gh:       0

       effective_chars = in_chars + out_chars + amortized_mcp_init_chars
       approx_tokens   = effective_chars / 4   # optional display only

     MCP init/context is real token usage for octocode and MUST be included.
     It includes server instructions and tool schemas loaded into context. gh
     has no equivalent schema-loading cost.

  C. QUALITY-ADJUSTED TOKEN SCORE (winner axis):

       token_score = quality / (effective_chars / 1000)

     Interpret as quality points per 1k measured characters. Higher is better.
     This is the benchmark's winner metric. Do NOT multiply by elapsed time.

     A cheaper wrong answer is not a win: if quality is 0, token_score is 0.
     If one agent has materially lower raw quality but higher token_score, call
     out the tradeoff explicitly. If raw quality differs by >=1 point on a Q,
     do not hide that behind token thrift; explain whether fewer chars bought a
     worse answer.

  D. HONESTY:
       - Did the agent claim a fact without evidence (invented line numbers,
         made-up PR titles, fabricated function names)?
       - Did the agent honestly say UNKNOWN when blocked, or hallucinate?
         Hallucinations are worse than UNKNOWN.

  PER-Q WINNER (non-drift only):
     A wins iff token_score(A) > token_score(B). If scores are within 5%, write
     `tie`. Drift Qs: write "—" (not in verdict).

═══════════════════════════════════════════════════════════════════
STEP 2 — Write the comparison to benchmark/github/output/summary.md
═══════════════════════════════════════════════════════════════════

Output path: the benchmark output dir — the parent of both run dirs:
  benchmark/github/output/summary.md    (i.e. $RUN_OCTO/../summary.md)

REQUIRED SECTIONS (in this order):

  # Benchmark summary — <octocode-run-slug> vs <gh-run-slug>

  Brief paragraph (3–5 sentences): which agents ran, on which questions, and
  the one-line headline: who won on raw quality and who won on
  quality-adjusted token usage.

  ## Per-question table

  | Q | Drift | Octo qual | gh qual | Octo chars | gh chars | Octo token score | gh token score | Winner | Notes |

  - Quality columns are YOUR semantic scores (0–3).
  - For drift Qs, prefix score with "d:" (e.g. "d:2/3") and mark Drift = ✓.
  - `Octo chars` and `gh chars` are effective chars for that Q: per-Q
    `in_chars + out_chars`, plus amortized MCP init for octocode.
  - `Octo token score` and `gh token score` are `quality / (effective_chars/1000)`.
  - Winner: token-score rule above. Drift Qs: "—".
  - Notes: one short clause, cite specific missing facts where useful.

  ## Quality verdict (non-drift Qs only)

  | Agent | Σ quality | Token-score wins | Token-score ties | Avg quality per Q |
  | octocode | X/3N | a | t | x.xx |
  | gh       | Y/3N | b | t | y.yy |

  2–3 sentences: which question categories each agent answered more accurately,
  which Qs were closest, and whether either agent's token win came with lower
  answer quality.

  ## Drift verdict (reported separately)

  | Agent | Σ drift quality |
  One-line note on which drift Qs neither agent answered well.

  ## Quality-adjusted token-usage verdict

  Pull totals from summary.json. MCP init = one-time per-session context cost
  (octocode only — gh has no schema loading step). Use character fields only.

  | Axis | octocode | gh | ratio (octo/gh) |
  | Σ quality (non-drift) |   |   | |
  | Σ calls               |   |   | |
  | Σ in_chars (per-Q)    |   |   | |
  | Σ out_chars (per-Q)   |   |   | |
  | MCP init chars        |   | 0 | |
  | TOTAL chars (per-Q + init) | | | |
  | Approx tokens (`TOTAL chars / 4`) | | | |
  | Quality per 1k chars = Σ quality / (TOTAL chars/1000) | | | |
  | Σ tool_elapsed_ms (context only) |   |   | |
  | Σ q_elapsed_ms (context only)    |   |   | |
  | Σ reasoning_ms (context only)    |   |   | |

  3–4 sentences interpreting the table. What fraction of octocode's total chars
  was init vs per-Q work? Which agent delivered more quality per measured token
  budget? Did either agent merely save chars by producing a worse or less useful
  answer? Let the numbers speak.

  ## Failure-mode review

  Bullet list of concrete issues found:
  - Hallucinated identifiers (which Q, which agent, what was wrong).
  - Q where an agent answered confidently but was wrong.
  - Q where UNKNOWN was the honest answer but the agent guessed.
  - Token-consumption pathologies (huge raw output, repeated schema/init cost,
    broad unfiltered dumps, or excessive calls).
  - Bad questions (ambiguous in QUESTIONS.md or too few verifiable facts).

  ## Verdict

  One paragraph, ≤4 sentences. State which agent won on **quality-adjusted token
  usage**, with caveats. State raw-quality winner separately. If raw quality is
  tied within ±2 points across all Qs, say so explicitly. If the token-score
  winner has materially lower quality, state that tradeoff plainly instead of
  hiding it behind the composite score.

═══════════════════════════════════════════════════════════════════
HARD RULES
═══════════════════════════════════════════════════════════════════

• For each Q, read the question and independently verify the answer facts before
  assigning scores. Do not let either agent's answer anchor your judgment.

• Cite a specific file path, identifier, PR discussion point, or agent claim for
  every non-3 score. Vague "incomplete" reasons don't count.

• Quote the agent verbatim when criticising an answer.

• If BOTH agents got something wrong, score both honestly.

• If a question is genuinely bad (ambiguous, or too few verifiable facts), flag
  it and EXCLUDE from the verdict totals.

• Include MCP init chars in octocode per-Q and total token usage. That cost is
  real.

• Do NOT choose the winner by wall-clock time or char-seconds. Time is context
  only. The winner metric is quality per measured token/character budget.

• Output ONE file: benchmark/github/output/summary.md (same as $RUN_OCTO/../summary.md). Do not write anything else.
  Do not chat — just write the file and stop.
```
