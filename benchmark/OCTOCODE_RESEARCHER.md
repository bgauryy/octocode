# Octocode Researcher — Agent Prompt

Operator: replace `<BENCHMARK>` with **`github`** or **`rtk`** before pasting.
Scoring model: see [`benchmark/README.md`](./README.md).

---

```
ROLE
Research agent. Answer benchmark/<BENCHMARK>/QUESTIONS.md in order
using only the Octocode CLI. Every tool call must go through the
metering wrapper — bare CLI calls are unmetered and disqualify the run.

═══════════════════════════════════════════════════════════════════
STEP 0 — LEARN YOUR TOOLS  (mandatory before any metered call)
═══════════════════════════════════════════════════════════════════

# From the repo root (development):
export OCTOCODE_CLI_BIN="packages/octocode-cli/out/octocode-cli.js"
# Installed globally: export OCTOCODE_CLI_BIN=$(which octocode-cli 2>/dev/null || which octocode)

Run and read the full output:
  node "$OCTOCODE_CLI_BIN" --tools-context

This lists every tool, its description, and every input field.
You must know this before calling anything. Key things to find:
  - required fields on each tool (mainResearchGoal, researchGoal, reasoning)
  - that 1–5 queries can be batched in one call
  - matchString / startLine / endLine for targeted file reads
  - evidence.answerReady and hints[] — follow them before retrying

Inspect one tool's schema at any time:
  node "$OCTOCODE_CLI_BIN" tools <tool-name>

═══════════════════════════════════════════════════════════════════
STEP 1 — SETUP
═══════════════════════════════════════════════════════════════════

source benchmark/<BENCHMARK>/scripts/init-run.sh octocode
  # creates output/octocode/, exports $RUN and $LOG

If BENCHMARK = rtk:
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
```
