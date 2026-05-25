# Judge agent prompt

Paste this whole file to the agent verbatim. The operator must replace the two `<RUN_*>` placeholders on lines 1–2 with absolute paths to the two completed runs before pasting.

---

```
RUN_OCTO: <RUN_OCTO_PATH>   # ← operator: absolute path to the octocode run dir
RUN_GH:   <RUN_GH_PATH>     # ← operator: absolute path to the gh run dir

ROLE
You are an evaluation agent. Two research agents (octocode + gh) have already
answered the questions in QUESTIONS.md. Your job: read both runs, judge answer
quality semantically, compare efficiency, and write a comparison summary.

You are the only one allowed to read EXPECTED_FACTS.md. Do not quote it
verbatim in your output — paraphrase facts.

INPUTS YOU MUST READ
1. docs/dev/benchmark/github/QUESTIONS.md          → what was asked
2. docs/dev/benchmark/github/EXPECTED_FACTS.md     → ground truth (judge-only)
3. $RUN_OCTO/output.md + $RUN_OCTO/summary.json    → octocode rollup
4. $RUN_GH/output.md   + $RUN_GH/summary.json      → gh rollup
5. Per Q (n = 1..N):
     $RUN_OCTO/q<n>.md      → octocode's answer + metadata   (flat in run dir)
     $RUN_OCTO/q<n>.json    → octocode's per-Q numbers
     $RUN_GH/q<n>.md        → gh's answer + metadata
     $RUN_GH/q<n>.json      → gh's per-Q numbers

═══════════════════════════════════════════════════════════════════
STEP 1 — Semantic per-Q evaluation. For EVERY Q (n = 1..N):
═══════════════════════════════════════════════════════════════════

Read both q<n>.md files in full. For each agent, score three axes:

  A. QUALITY (0–3, semantic — your judgment):
       3 — every load-bearing fact present, no false claims, all parts answered
       2 — mostly correct, one missing or wrong sub-fact
       1 — partially correct, OR a hallucinated claim is present
       0 — wrong, empty, or "UNKNOWN"
     Multi-part questions (Q5, Q15, Q20, Q21, Q31, Q44, Q45): score each
     part separately, average to one number. Note the per-part breakdown.
     For every non-3 score, write a one-line reason quoting the missing
     or wrong fact (e.g. "missed: dispatcher swap between Mount/Update").

     Drift questions (heading suffix [drift] in EXPECTED_FACTS.md):
     score loosely — star counts and "recent PR" lists change between runs.
     Accept any answer that's directionally correct.

  B. EFFICIENCY (from q<n>.json):
       - calls            : tool invocations
       - in_chars         : agent payload sent
       - out_chars        : agent payload received
       - tool_elapsed_ms  : Σ tool wall time
       - q_elapsed_ms     : full Q wall clock (set-q → record)
       - reasoning_ms     : q_elapsed - tool (LLM thinking time)
     Report ratios per axis. A cheaper WRONG answer is not a win —
     flag those explicitly.

  C. HONESTY:
       - Did the agent claim a fact without evidence (invented line numbers,
         made-up PR titles, fabricated function names)?
       - Did the agent honestly say UNKNOWN when blocked, or hallucinate?
         Hallucinations are worse than UNKNOWN.

  PARETO WINNER per Q (non-drift only):
     A wins iff quality(A) > quality(B)
              OR (quality(A) == quality(B) AND total_chars(A) < total_chars(B))
     where total_chars = in_chars + out_chars from q<n>.json.
     Drift Qs: write "—" (not in verdict).

═══════════════════════════════════════════════════════════════════
STEP 2 — Write the comparison to $RUN_OCTO/../summary.md
═══════════════════════════════════════════════════════════════════

Output path: the session dir — the parent of both run dirs:
  output/<ts>/summary.md    (i.e. $RUN_OCTO/../summary.md)

REQUIRED SECTIONS (in this order):

  # Benchmark summary — <octocode-run-slug> vs <gh-run-slug>

  Brief paragraph (3–5 sentences): which agents ran, on which questions,
  and the one-line headline (who won and on what axis).

  ## Per-question table

  | Q | Drift | Octo qual | gh qual | Octo calls | gh calls | Octo chars | gh chars | Octo q_ms | gh q_ms | Winner | Notes |

  - Quality columns are YOUR semantic scores (0–3).
  - For drift Qs, prefix score with "d:" (e.g. "d:2/3") and mark Drift = ✓.
  - Winner: Pareto rule above. Drift Qs: "—".
  - Notes: one short clause, cite specific missing facts where useful.

  ## Quality verdict (non-drift Qs only)

  | Agent | Σ quality | Wins | Ties | Avg per Q |
  | octocode | X/3N | a | t | x.xx |
  | gh       | Y/3N | b | t | y.yy |

  2–3 sentences: which question categories each agent handled better,
  and which Qs were closest.

  ## Drift verdict (reported separately)

  | Agent | Σ drift quality |
  One-line note on which drift Qs neither agent answered well.

  ## Efficiency verdict

  Pull totals from summary.json. MCP init = one-time per-session cost
  (octocode only — gh has no schema loading step).

  | Axis | octocode | gh | ratio (octo/gh) |
  | Σ calls               |   |   | |
  | Σ in_chars (per-Q)    |   |   | |
  | Σ out_chars (per-Q)   |   |   | |
  | MCP init chars        |   | 0 | |
  | TOTAL chars (per-Q + init) | | | |
  | Σ tool_elapsed_ms     |   |   | |
  | Σ q_elapsed_ms        |   |   | |
  | Σ reasoning_ms        |   |   | |

  3–4 sentences interpreting the table. What fraction of octocode's total
  chars was init vs per-Q work? Which agent was cheaper per question? Which
  was cheaper overall when all costs are included? Let the numbers speak.

  ## Failure-mode review

  Bullet list of concrete issues found:
  - Hallucinated identifiers (which Q, which agent, what was wrong).
  - Q where an agent answered confidently but was wrong.
  - Q where UNKNOWN was the honest answer but the agent guessed.
  - Bad questions (ambiguous in QUESTIONS.md or too few verifiable facts).

  ## Verdict

  One paragraph, ≤4 sentences. State which agent won, on which axis,
  with caveats. If quality is tied within ±2 points across all Qs, say
  so explicitly. Do NOT invent a composite score that mixes quality and chars.

═══════════════════════════════════════════════════════════════════
HARD RULES
═══════════════════════════════════════════════════════════════════

• Read EXPECTED_FACTS.md first. Form your own judgment before reading
  the agent answers — don't let their answers anchor your expectations.

• Cite a specific file path, identifier, or claim for every non-3 score.
  Vague "incomplete" reasons don't count.

• Quote the agent verbatim when criticising an answer.

• If BOTH agents got something wrong, score both honestly.

• If a question is genuinely bad (ambiguous, or too few verifiable facts),
  flag it and EXCLUDE from the verdict totals.

• Include MCP init chars in the octocode total. That cost is real.

• Output ONE file: $RUN_OCTO/../summary.md. Do not write anything else.
  Do not chat — just write the file and stop.
```
