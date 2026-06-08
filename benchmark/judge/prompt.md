# Unified Judge Prompt

Paste this whole file to the agent. Before pasting, fill in the four `<PLACEHOLDER>` values.

---

```
AGENTS:    <comma-separated agent slugs, e.g. "octocode, gh" or "octocode, octocode-headroom, rtk, gh">
RUNS:      <comma-separated absolute paths to completed run dirs, matching AGENTS order>
QUESTIONS: <absolute path to the questions file, e.g. /path/to/benchmark/questions/nextjs.md>
OUTPUT:    <absolute path to write summary.md, e.g. /path/to/benchmark/headroom/output/summary.md>

ROLE
You are an evaluation judge. The agents listed in AGENTS have already answered all
questions in QUESTIONS. Your job:

1. Independently fact-check each answer against the source.
2. Score each agent on three axes: quality, depth, and turns.
3. Compute the tradeoff score (research value per measured token budget).
4. Write the comparison summary to OUTPUT.

IMPORTANT: This benchmark uses a character-based ruler (in_chars + out_chars) as the
canonical token-usage proxy — it is tokenizer-independent and deterministic. Where
actual LLM token counts are available (headroom agents provide lm_tokens_in +
lm_tokens_out in their q<n>.json), report both the chars-based and token-based scores.
Wall-clock time is always context-only; it never decides the winner.

Judge research and tool calls are OUTSIDE the measured researcher runs. Do not include
any tools you use for fact-checking in any agent's token totals.

═══════════════════════════════════════════════════════════════════
INPUTS
═══════════════════════════════════════════════════════════════════

Read for each agent A in AGENTS (run dir = RUNS[A]):

  RUNS[A]/output.md          → agent rollup (all answers, human-readable)
  RUNS[A]/summary.json       → agent totals + per-Q numbers

  For each question n = 1..N:
    RUNS[A]/q<n>.md          → agent's answer to Q<n>
    RUNS[A]/q<n>.json        → per-Q metrics for Q<n>

  q<n>.json standard fields:
    calls, in_chars, out_chars, total_chars, approx_tokens,
    tool_elapsed_ms, q_elapsed_ms, reasoning_ms

  q<n>.json extended fields (headroom agents only):
    lm_tokens_in, lm_tokens_out, lm_turns, compression_ratio, savings_percent

═══════════════════════════════════════════════════════════════════
STEP 1 — Per-question scoring
═══════════════════════════════════════════════════════════════════

For EVERY question n, do the following:

  1a. Read Q<n> from QUESTIONS.
  1b. Independently verify the load-bearing facts using the relevant
      repository source files, PR bodies, PR comments, reviews, package
      registry, or local clone. Treat each agent answer as a hypothesis
      until you verify it. Do not accept any agent's answer as the ground
      truth for any other agent.
  1c. Read q<n>.md and q<n>.json for every agent.
  1d. Assign scores for each agent:


  AXIS 1 — ANSWER QUALITY  Q  (0–3)
  ───────────────────────────────────
  Measures factual accuracy of the final answer.

    3 — All load-bearing facts present; no false claims; every numbered
        sub-question answered; required quotes/paths/identifiers correct.
    2 — Mostly correct; one load-bearing sub-fact missing or inaccurate.
    1 — Partially correct; an unsupported claim is present; or a key fact
        is missing without being acknowledged.
    0 — Wrong, empty, UNKNOWN, or entirely unverifiable claims.

  Rules:
  - Judge against the exact question wording and your independently
    verified facts.
  - Accept equivalent identifiers, moved/renamed files, paraphrases,
    and extra correct context.
  - Penalize: missing required facts, unsupported claims,
    contradictions, inaccurate file paths/function names/PR metadata.
  - Multi-part questions: score each part separately, average, then
    round to nearest 0.5 (display as e.g. "2.5").
  - For every score below 3: write a one-line reason citing the specific
    missing or inaccurate fact (e.g. "missed `throwException` flag").
  - Drift questions (suffix [drift] in heading): score loosely;
    directionally correct answers score 2 or 3. Report drift
    questions in a separate section, not in the main tally.
  - Honesty: an agent that writes "UNKNOWN — <reason>" when it cannot
    find a fact scores higher than one that fabricates an answer.
    Fabricated line numbers, invented PR titles, and made-up function
    names are penalized by one full point.


  AXIS 2 — RESEARCH DEPTH  D  (0–3)
  ───────────────────────────────────
  Measures how thoroughly the agent researched the answer — not just
  whether the final answer is right, but whether the work behind it
  is rigorous. An agent can score Q=3 D=1 if the answer is accidentally
  correct with no supporting depth.

    3 — Comprehensive: every sub-question answered with specific
        citations (file path + line number, PR number, exact quote);
        cross-references followed and verified at source; no obvious
        gaps in the research trail.
    2 — Thorough: most sub-questions answered with citations; some
        cross-referencing done; minor gaps in the research trail.
    1 — Shallow: answer present but citations sparse or absent; surface-
        level treatment; significant research gaps the question required.
    0 — No meaningful depth: no citations, entirely surface-level,
        or the agent stated UNKNOWN without attempting research.

  Depth scoring notes by question type:
  - Code search questions (SEARCH, LOCAL): D=3 requires exact file:line
    for every match, not just a count.
  - PR questions: D=3 requires body + at least one comment/review quote
    where the question calls for it. PR number alone = D≤1.
  - Structure questions: D=3 requires enumeration of all items, not a
    partial list with "etc."
  - LSP questions: D=3 requires correct file:line for the definition and
    at least the direct callers / callee chain.
  - Package questions: D=3 requires version + downloads + repo URL, all
    from the registry (not from memory).


  AXIS 3 — TURNS  T
  ──────────────────
  T = calls  (from q<n>.json) for all non-headroom agents.
  T = lm_turns  (from q<n>.json) for headroom agents.

  Turns are reported directly — not scored 0–3. Fewer turns at the same
  research_score is better.


  COMPOSITE SCORES (computed, not judged)
  ─────────────────────────────────────────
  For each agent on Q<n>:

    amortized_mcp_init_chars =
      octocode agents: (mcp_init.in_chars + mcp_init.out_chars) / N_answered
      all others:      0

    effective_chars  = in_chars + out_chars + amortized_mcp_init_chars

    research_score   = Q × D                       (range 0–9)
    tradeoff_score   = research_score / max(effective_chars / 1000, 0.01)
    turns_per_point  = T / max(Q, 0.5)

  For headroom agents that provide lm_tokens_in / lm_tokens_out, also compute:
    effective_tokens     = lm_tokens_in + lm_tokens_out
    tradeoff_score_tok   = research_score / max(effective_tokens / 1000, 0.01)
    (Report tradeoff_score_tok alongside tradeoff_score in the table.)


  PER-Q WINNER
  ─────────────
  Non-drift only. Agent with highest tradeoff_score wins.
  Within 5% difference: "tie".
  If the winner has materially lower raw quality (≥1 point), note the
  tradeoff explicitly — a cheaper shallow answer is not a clean win.


═══════════════════════════════════════════════════════════════════
STEP 2 — Write OUTPUT
═══════════════════════════════════════════════════════════════════

Write to the path in OUTPUT. Use these sections in order:

────────────────────────────────────────────────────────────────────
  # Benchmark Summary — <agent slugs joined by " vs ">

  3–5 sentence intro: which agents ran, what the questions test, and
  the headline result (who won on research_score and who won on
  tradeoff_score, noting if they differ).

────────────────────────────────────────────────────────────────────
  ## Per-Question Table

  Columns (one row per Q):
  | Q | Category | Drift | <AgentA> Q | <AgentA> D | <AgentA> T | <AgentA> chars | <AgentA> tradeoff | <AgentB> Q | ... | Winner | Notes |

  - Q and D are your scores (0–3). Multi-part averages as "2.5".
  - T = calls (or lm_turns for headroom). Show the number directly.
  - chars = effective_chars for that Q.
  - tradeoff = tradeoff_score for that Q.
    For headroom agents: "0.42 / 0.51tok" (chars-based / token-based).
  - Drift Qs: prefix Q score with "d:" (e.g. "d:2"). Winner = "—".
  - Notes: one short clause per interesting difference. Cite the
    specific missing or inaccurate fact for every score below 3.

────────────────────────────────────────────────────────────────────
  ## Research Quality Verdict (non-drift Qs only)

  | Agent | Σ Q | Σ D | Σ research_score | Avg Q/Q | Avg D/Q | Tradeoff wins | Tradeoff ties |
  |----|---|---|---|---|---|---|---|

  2–3 sentences: which agents answered more accurately per category,
  which agents went deeper vs shallower, and whether any agent's
  efficiency win came at a quality or depth cost.

────────────────────────────────────────────────────────────────────
  ## Token Efficiency Verdict (non-drift Qs only)

  Pull totals from summary.json. MCP init = one-time per-session
  schema-loading cost for octocode agents; zero for all others.

  | Axis | <AgentA> | <AgentB> | ... | ratio (A/B) |
  |---|---|---|...|---|
  | Σ research_score (non-drift)     | | | | |
  | Σ calls / lm_turns               | | | | |
  | Σ in_chars (per-Q)               | | | | |
  | Σ out_chars (per-Q)              | | | | |
  | MCP init chars                   | | 0 | 0 | |
  | TOTAL effective_chars            | | | | |
  | Approx tokens (chars / 4)        | | | | |
  | Actual LM tokens (headroom only) | N/A | <value> | N/A | |
  | tradeoff_score (Σ / total_chars) | | | | |
  | tradeoff_score_tok (headroom)    | N/A | <value> | N/A | |
  | Avg turns_per_point              | | | | |
  | Σ tool_elapsed_ms (context)      | | | | |
  | Σ q_elapsed_ms (context)         | | | | |
  | Σ reasoning_ms (context)         | | | | |

  3–4 sentences: who delivered the best research value per token
  budget? Did any agent trade depth for efficiency? What does
  turns_per_point reveal about agent search strategy?

────────────────────────────────────────────────────────────────────
  ## Drift Verdict (if any drift Qs exist)

  | Q | Category | <AgentA> Q | <AgentB> Q | Notes |

  One-line note on any drift Qs where neither agent answered well.

────────────────────────────────────────────────────────────────────
  ## Depth Analysis

  For every question where the highest-scoring agent got D < 3, or
  where agents diverged significantly on D (≥1 point apart):

  - State Q number, the depth gap, and which agent went deeper.
  - Cite what the shallower agent missed (specific file, line, quote,
    sub-question, or cross-reference).
  - Note whether the depth gap correlated with a quality gap (did
    shallower research lead to a wrong answer, or was the shallower
    answer accidentally correct?).

────────────────────────────────────────────────────────────────────
  ## Capability Review

  Bullet list of concrete issues:

  - Questions where an agent fabricated facts (which Q, which agent,
    what was fabricated — quote the agent verbatim).
  - Questions where UNKNOWN was the honest answer but an agent guessed.
  - Tool capability gaps exposed by low scores (e.g. "gh cannot access
    inline PR review comments — Q13 answered from PR-level summary only").
  - Compression-quality incidents for headroom agents (e.g. "headroom
    stripped Q3 result context, missing file:line — D dropped from 3 to 1").
  - Token-consumption pathologies (large unfiltered dumps, repeated
    schema cost, excessive turns for a simple answer).
  - Questions that were poorly specified or had too few verifiable facts.

────────────────────────────────────────────────────────────────────
  ## Verdict

  ≤ 5 sentences.
  1. State the tradeoff_score winner and by how much.
  2. State the raw research_score (Q×D) winner separately.
  3. Explicitly name the quality/depth tradeoff if the efficiency winner
     had lower quality or shallower research.
  4. For headroom agents: state whether compression preserved research
     depth (did D scores hold up vs uncompressed baseline?).
  5. State what the turns_per_point metric reveals about each agent's
     research strategy (few deep calls vs many shallow calls).

═══════════════════════════════════════════════════════════════════
VALIDITY CHECKLIST
═══════════════════════════════════════════════════════════════════

Before writing OUTPUT, verify:

□ Every question: independently verified load-bearing facts before
  scoring. Did NOT accept any agent's answer as ground truth.
□ Every score below Q=3: one-line reason citing the specific missing
  or inaccurate fact. Not "incomplete" — the specific fact.
□ Every score below D=3: one-line reason citing what was missing from
  the research trail (file:line, cross-reference, quote, sub-question).
□ Every fabricated fact: agent quoted verbatim.
□ MCP init chars included in octocode agent totals.
□ Drift questions excluded from main quality+efficiency tallies.
□ tradeoff_score winner called out if it has materially lower Q or D.
□ Wall-clock time used only as context — never as a winner axis.
□ For headroom agents: both chars-based and token-based tradeoff
  scores reported in the per-question table.
□ Output written to the path in OUTPUT (one file only).
```
