# Octocode Researcher Prompt

Paste this prompt to an Octocode benchmark researcher.

Operator setup:

- Replace `<BENCHMARK>` with `github` or `rtk`.
- Use only suites that have `benchmark/<BENCHMARK>/QUESTIONS.md` and `benchmark/<BENCHMARK>/scripts/`.
- For shared question banks such as `benchmark/questions/nextjs.md`, create or select a runnable suite harness first.

Scoring model: see [`benchmark/COMPARISON.md`](./COMPARISON.md).

---

```
ROLE
Research agent. Answer benchmark/<BENCHMARK>/QUESTIONS.md in order
using only the Octocode CLI. Every tool call must go through the
metering wrapper — bare CLI calls are unmetered and disqualify the run.

VALID BENCHMARK VALUES
  github — GitHub API research suite
  rtk    — local + GitHub research suite against rtk-ai/rtk

═══════════════════════════════════════════════════════════════════
STEP 0 — LEARN YOUR TOOLS  (mandatory before any metered call)
═══════════════════════════════════════════════════════════════════

# From the repo root, development checkout:
export OCTOCODE_CLI_BIN="packages/octocode-cli/out/octocode-cli.js"
# Installed globally: export OCTOCODE_CLI_BIN=$(which octocode-cli 2>/dev/null || which octocode)

Run and read the agent protocol:
  node "$OCTOCODE_CLI_BIN" --agent

If you need every schema inline, run:
  node "$OCTOCODE_CLI_BIN" --agent --full

This lists every tool, its description, and the required calling protocol.
You must understand this before any metered tool call. Key things to find:
  - required fields on each tool (mainResearchGoal, researchGoal, reasoning)
  - that 1–5 queries can be batched in one call
  - matchString / startLine / endLine for targeted file reads
  - evidence.answerReady and hints[] — follow them before retrying

Inspect one tool's schema at any time:
  node "$OCTOCODE_CLI_BIN" tools <tool-name>

Inspect one tool's full MCP-style JSON schema:
  node "$OCTOCODE_CLI_BIN" tools <tool-name> --format tool

═══════════════════════════════════════════════════════════════════
STEP 1 — SETUP
═══════════════════════════════════════════════════════════════════

rm -rf benchmark/<BENCHMARK>/output/octocode
source benchmark/<BENCHMARK>/scripts/init-run.sh octocode
  # creates output/octocode/, exports $RUN and $LOG

If BENCHMARK = rtk:
  rm -rf /tmp/rtk-bench
  git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench

═══════════════════════════════════════════════════════════════════
STEP 2 — CALL FORMAT
═══════════════════════════════════════════════════════════════════

  bash benchmark/<BENCHMARK>/scripts/octo-meas.sh <tool-name> '<queries-json>'

queries-json shape:
  {
    "queries": [{
      "id": "1",
      "mainResearchGoal": "...",
      "researchGoal": "...",
      "reasoning": "...",
      <tool-specific fields>
    }]
  }

Batch example (two lookups, one metered entry):
  bash benchmark/<BENCHMARK>/scripts/octo-meas.sh githubSearchCode '{
    "queries": [
      {"id":"1","mainResearchGoal":"find state primitives","researchGoal":"locate useState","reasoning":"need definition file","keywordsToSearch":["useState"],"owner":"facebook","repo":"react"},
      {"id":"2","mainResearchGoal":"find state primitives","researchGoal":"locate createSignal","reasoning":"need definition file","keywordsToSearch":["createSignal"],"owner":"solidjs","repo":"solid"}
    ]
  }'

Targeted read (matchString instead of fullContent):
  bash benchmark/<BENCHMARK>/scripts/octo-meas.sh localGetFileContent '{
    "queries": [{"id":"1","mainResearchGoal":"...","researchGoal":"...","reasoning":"...","path":"/tmp/rtk-bench/src/core/runner.rs","matchString":"skip_filter_on_failure"}]
  }'

Verify attribution after the first call for Q<n>:
  grep '"q":<n>' "$RUN/log.jsonl"

Output guidance:
  - the benchmark wrapper returns the default Octocode tool output
  - read the returned evidence and hints directly
  - never discard hints[]; they are part of the tool contract

═══════════════════════════════════════════════════════════════════
STEP 3 — PER-QUESTION LOOP
═══════════════════════════════════════════════════════════════════

For n = 1 to N  (N = `cat $RUN/.q-count`):

  1. bash benchmark/<BENCHMARK>/scripts/set-q.sh <n>
     — run BEFORE any tool call

  2. awk -v q="<n>" '$0~"^### Q"q" —"{p=1;print;next} p&&/^### Q[0-9]+ —/{exit} p{print}' \
       benchmark/<BENCHMARK>/QUESTIONS.md

  3. Research with metered calls. Batch independent lookups.
     Follow hints[] in every response before retrying.

  4. Write /tmp/answer.md:
     - First bullet directly — no "## Answer" header
     - One bullet per fact or sub-question
     - Paths, names, PR numbers, identifiers in backticks
     - Completeness over brevity
     - Unanswerable: UNKNOWN — <one-line reason>
     - Do not include tool transcripts or process notes

  5. bash benchmark/<BENCHMARK>/scripts/record.sh <n> "<model-id>" /tmp/answer.md
     — never use --allow-zero; zero rows = metering failure, redo

═══════════════════════════════════════════════════════════════════
STEP 4 — FINALISE
═══════════════════════════════════════════════════════════════════

  node benchmark/<BENCHMARK>/scripts/finalize.mjs "$RUN"

═══════════════════════════════════════════════════════════════════
RULES
═══════════════════════════════════════════════════════════════════

• set-q.sh before first tool call each question
• all calls through octo-meas.sh — bare node calls are unmetered
• sequential — finish Q<n> before Q<n+1>
• no --allow-zero
• blind — do not read the other agent's output or summary.md
• do not use benchmark/questions/*.md directly unless a suite harness points at it
```
