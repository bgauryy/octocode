# Researcher agent prompt

For the **octocode researcher**: use [`benchmark/OCTOCODE_RESEARCHER.md`](../OCTOCODE_RESEARCHER.md) — set `<BENCHMARK>` to `github` before pasting.

For the **gh researcher**: paste this file with `<TOOLSET>` set to `gh`.

---

```
TOOLSET: <TOOLSET>          # ← operator: set to either "octocode" or "gh"

ROLE
You are a research agent. Answer the questions in QUESTIONS.md one by one,
in order, using the toolset declared above. Every tool call is metered.

QUESTION-FIRST GOAL
For each question, read the exact wording first and answer what it asks — not a
fixed rubric you imagine. Identify the requested deliverables before searching
(e.g. repos, functions, files, PR discussion points, comparison axes), then use
as many metered calls as needed to produce the best supported answer. Continue
beyond the first plausible hit when the question asks for multiple repos, a
trace, or several sub-questions.

INPUT FILES
- benchmark/github/QUESTIONS.md   → the questions to answer
- benchmark/github/scripts/       → the metering scripts (see below)

KEEP THE RUN BLIND
- benchmark/github/README.md           → operator/reviewer view only.
- benchmark/github/output/summary.md   → final judge output, if it exists.
- the other researcher run directory   → keeps the two research runs blind.

═══════════════════════════════════════════════════════════════════
ALLOWED TOOLS — branches on TOOLSET
═══════════════════════════════════════════════════════════════════

IF TOOLSET = octocode:
  Call Octocode tools through the metering wrapper:
    bash benchmark/github/scripts/octo-meas.sh <tool-name> '<queries-json>'

  Examples:
    bash benchmark/github/scripts/octo-meas.sh githubSearchCode \
      '{"keywordsToSearch":["renderToReadableStream"],"owner":"vercel","repo":"next.js"}'
    bash benchmark/github/scripts/octo-meas.sh githubGetFileContent \
      '{"owner":"facebook","repo":"react","path":"packages/react/src/ReactHooks.js"}'
    bash benchmark/github/scripts/octo-meas.sh githubViewRepoStructure \
      '{"owner":"vercel","repo":"next.js","path":"packages"}'
    bash benchmark/github/scripts/octo-meas.sh githubSearchPullRequests \
      '{"owner":"facebook","repo":"react","query":"concurrent mode"}'
    bash benchmark/github/scripts/octo-meas.sh githubSearchRepositories \
      '{"keywordsToSearch":["react state management"]}'

  Valid Octocode calls appear in $RUN/log.jsonl.

  Outside this run: bare `octocode tools ...` (without the wrapper),
                    gh CLI, web search, curl/fetch/wget, git clone, any
                    other MCP server, reading local repo files.

IF TOOLSET = gh:
  Run `gh` CLI commands through the wrapper:
    bash benchmark/github/scripts/gh-meas.sh <gh-args>
  Every gh call goes through that wrapper so it is metered. The wrapper accepts
  any valid `gh` sub-command and flags.
  Outside this run: bare `gh ...`, any octocode tool, web search,
                    curl/fetch/wget, git clone, reading local repo files.

═══════════════════════════════════════════════════════════════════
SETUP — operator runs ONCE before the agent loop starts
═══════════════════════════════════════════════════════════════════

source benchmark/github/scripts/init-run.sh <TOOLSET>
  # Creates benchmark/github/output/<TOOLSET>/ ($RUN)
  # and exports $SESSION=benchmark/github/output, $RUN, $LOG, $Q=0.
  # Remove an existing output/<TOOLSET>/ before starting a fresh run.

IF TOOLSET = octocode — configure the local CLI binary (required):
  export OCTOCODE_CLI_BIN=/Users/guybary/Documents/octocode-mcp/packages/octocode-cli/out/octocode-cli.js

  MANDATORY pre-flight: review the agent protocol, tool descriptions, schemas,
  and bulk-query patterns before making any metered call. This ensures correct
  parameter usage and fewer wasted calls (directly improves token counts):
    node "$OCTOCODE_CLI_BIN" --agent
    node "$OCTOCODE_CLI_BIN" --agent --full

  Read the full output. Key patterns to note:
    - Every tool accepts 1–5 queries per call — batch independent lookups.
    - mainResearchGoal / researchGoal / reasoning are required on every query.
    - Use matchString or startLine/endLine for targeted file reads (not fullContent).
    - Trust evidence.answerReady / hints in each result as required next steps.

═══════════════════════════════════════════════════════════════════
PER-QUESTION LOOP — sequential, one question at a time
═══════════════════════════════════════════════════════════════════

For each n from 1 to N (where N = `cat $RUN/.q-count`):

  1. bash benchmark/github/scripts/set-q.sh <n>
     Run this BEFORE making any tool call for Q<n>. It writes <n> to
     .current-q (routes subsequent tool calls to this question in the log)
     and records the start timestamp for q_elapsed_ms.

  2. Read the question:
     awk -v q="<n>" '
       $0 ~ "^### Q"q" —" { p=1; print; next }
       p && /^### Q[0-9]+ —/  { exit }
       p { print }
     ' benchmark/github/QUESTIONS.md

  3. Research using only your assigned toolset. Answer the question as
     accurately as you can. Some questions span multiple repositories or
     require reading several files; use as many tool calls as the question
     needs. Your answer should be evidence-backed: prefer concrete repo slugs,
     paths, function/component names, APIs called, and PR review facts over
     generic summaries.

     After the first tool call for Q<n>, confirm that $RUN/log.jsonl gained
     a row with "q":<n>. If not, pause and fix the metered path before
     recording an answer.

  4. Write your answer to /tmp/answer.md using this format:
       - Start directly with the first bullet. Omit a `## Answer`
         header — record.sh adds it.
       - Use concise bullets, but there is NO fixed line limit. Completeness
         beats forced brevity; include every load-bearing fact needed to answer
         the question.
       - Use one bullet per fact or per requested sub-part. For numbered
         questions, prefix bullets with `1.`, `2.`, etc. For multi-repo
         questions, use one bullet per repo plus a final comparison bullet when
         the question asks for architecture/tradeoffs.
       - Every file path, repo slug, function name, PR number, version string,
         API name, and important identifier should be in BACKTICKS when practical.
       - Multi-cap identifiers (`ReactSharedInternals`, `HooksDispatcherOnMount`)
         may be bare but must be verbatim, exact case.
       - Facts only: no narration, no explanation of your process, no tool-call
         transcript, no speculation.
       - If you cannot answer after appropriate metered research: write
         `UNKNOWN — <one-line reason>`. Use UNKNOWN rather than guessing.

  5. bash benchmark/github/scripts/record.sh <n> "<your-model-id>" /tmp/answer.md
     Aggregates $LOG for Q<n>, computes q_elapsed_ms, writes
     $RUN/q<n>.json (canonical metrics) and $RUN/q<n>.md (human view).

     Leave `--allow-zero` unused for benchmark runs.
     If record.sh reports "zero rows for q=<n>", redo the question through the
     metered path. Delete any invalid Q output if present, reconfigure the tool
     path, and redo the question through the required wrapper before moving on.

  6. Move to n+1.

═══════════════════════════════════════════════════════════════════
FINALISE — after the last question is recorded
═══════════════════════════════════════════════════════════════════

node benchmark/github/scripts/finalize.mjs "$RUN"
  # writes $RUN/output.md  (human summary)
  # writes $RUN/summary.json  (machine sidecar for the judge)

═══════════════════════════════════════════════════════════════════
VALIDITY CHECKLIST
═══════════════════════════════════════════════════════════════════

• Run set-q.sh BEFORE the first tool call for each question.
  Skipping it misattributes calls in the log.

• Sequential: finish Q<n> (including record.sh) before Q<n+1>.

• Use the assigned toolset for the run. Mixing tools makes the measurement unusable.

• Octocode: every call must go through octo-meas.sh. Bare `octocode tools`
  is unmetered.

• Gh: every gh call must go through gh-meas.sh. Bare `gh` is unmetered.

• Leave `--allow-zero` unused for benchmark agent runs. A zero-row question is
  a metering failure, not a successful answer.

• If a question cannot be answered (tool error, rate limit, genuinely
  unavailable data), write `UNKNOWN — <one-line reason>`.

• Keep the run blind until finalized: leave the other agent's output and
  benchmark/github/output/summary.md unread.

• Keep process narration out of the space between questions.
  Only the recorded output matters.
```
