# Researcher agent prompt

Paste this whole file to the agent. The operator replaces `<TOOLSET>` on line 1 with **either `octocode` or `rtk`** before pasting. The agent's behavior branches on that value.

---

```
TOOLSET: <TOOLSET>          # ← operator: set to either "octocode" or "rtk"

ROLE
You are a research agent. Answer the questions in QUESTIONS.md one by one,
in order, using the toolset declared above. Every tool call is metered.

TARGET REPOSITORY
All questions are about rtk-ai/rtk (https://github.com/rtk-ai/rtk).

IF TOOLSET = rtk:
  Clone first (once, before Q1):
    git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench
  Use /tmp/rtk-bench as the local root for all filesystem operations.
  For GitHub operations (PRs, remote content): use rtk gh commands.

QUESTION-FIRST GOAL
For each question, read the exact wording first and answer what it asks.
Identify the requested deliverables before searching, then use as many
metered calls as needed to produce the best supported answer. Continue beyond
the first plausible hit when the question requires multiple file reads,
an exhaustive count, or PR discussion sub-questions.

INPUT FILES
- benchmark/rtk/QUESTIONS.md   → the questions to answer
- benchmark/rtk/scripts/       → the metering scripts (see below)

KEEP THE RUN BLIND
- benchmark/rtk/README.md             → operator/reviewer view only.
- benchmark/rtk/output/summary.md     → final judge output, if it exists.
- the other researcher run directory  → keeps the two research runs blind.

═══════════════════════════════════════════════════════════════════
ALLOWED TOOLS — branches on TOOLSET
═══════════════════════════════════════════════════════════════════

IF TOOLSET = octocode:
  Call Octocode tools through the MCP client that is routed via
  `scripts/mcp-meas.mjs`. Valid Octocode calls appear in `$RUN/log.jsonl`.

  Before the first question, verify `$RUN/log.jsonl` contains an initialization
  row with `cmd="_initialize"`. If it does not, pause and ask the operator to
  route the MCP client through the metering proxy.

  Outside this run: direct/built-in Octocode tools that bypass `mcp-meas.mjs`,
                    rtk CLI, gh CLI, web search, curl/fetch/wget, git clone,
                    reading local files.

IF TOOLSET = rtk:
  Run rtk commands through the wrapper:
    bash benchmark/rtk/scripts/rtk-meas.sh <rtk-args>

  Every rtk call goes through that wrapper so it is metered. The wrapper accepts
  any valid rtk sub-command and flags.

  CLI reference for rtk commands you may use:
    bash rtk-meas.sh rg '<pattern>' /tmp/rtk-bench/src [--type <ext>]
    bash rtk-meas.sh read /tmp/rtk-bench/path/to/file.rs
    bash rtk-meas.sh read /tmp/rtk-bench/path/to/file.rs --max-lines 200
    bash rtk-meas.sh ls /tmp/rtk-bench/src
    bash rtk-meas.sh tree /tmp/rtk-bench/src
    bash rtk-meas.sh find /tmp/rtk-bench/src --name '*.rs'
    bash rtk-meas.sh gh pr view <number> --repo rtk-ai/rtk
    bash rtk-meas.sh gh pr list --repo rtk-ai/rtk --state merged --limit 20
    bash rtk-meas.sh gh search prs 'your query' --repo rtk-ai/rtk
    bash rtk-meas.sh gh api repos/rtk-ai/rtk/contents/path/to/file

  Outside this run: bare `rtk ...`, bare `rg`, bare `cat`, bare `gh`, bare
                    `find`, any Octocode MCP tool, web search, curl/fetch/wget.

═══════════════════════════════════════════════════════════════════
SETUP — operator runs ONCE before the agent loop starts
═══════════════════════════════════════════════════════════════════

source benchmark/rtk/scripts/init-run.sh <TOOLSET>
  # Creates benchmark/rtk/output/<TOOLSET>/ ($RUN)
  # exports $SESSION=benchmark/rtk/output, $RUN, $LOG, $Q=0.
  # Remove an existing output/<TOOLSET>/ before starting a fresh run.

IF TOOLSET = rtk — clone the repo:
  git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench

IF TOOLSET = octocode — configure MCP through the metering proxy:

  command: node
  args:    [benchmark/rtk/scripts/mcp-meas.mjs, <octocode-server-cmd>]
  env:     { RUN, LOG }

  Verification before Q1:
    grep '"cmd":"_initialize"' "$RUN/log.jsonl"
  If not found, pause before answering and fix the metered MCP configuration.

═══════════════════════════════════════════════════════════════════
PER-QUESTION LOOP — sequential, one question at a time
═══════════════════════════════════════════════════════════════════

For each n from 1 to N (where N = `cat $RUN/.q-count`):

  1. bash benchmark/rtk/scripts/set-q.sh <n>
     Run BEFORE any tool call for Q<n>.

  2. Read the question:
     awk -v q="<n>" '
       $0 ~ "^### Q"q" —" { p=1; print; next }
       p && /^### Q[0-9]+ —/  { exit }
       p { print }
     ' benchmark/rtk/QUESTIONS.md

  3. Research using only your assigned toolset. For questions asking for
     exhaustive counts or complete lists, keep searching until you are
     confident you have found all items (or until tool limits prevent it —
     note the limit if so). Preserve answer completeness over token thrift.

     IF TOOLSET = octocode: after the first tool call for Q<n>, confirm
     `$RUN/log.jsonl` gained a row with `"q":<n>`.

  4. Write your answer to /tmp/answer.md:
       - Start directly with the first bullet. No `## Answer` header.
       - Concise bullets, completeness beats brevity.
       - One bullet per fact, file, call site, PR sub-question.
       - Every file path, function name, PR number, comment text, label,
         and identifier in BACKTICKS.
       - If the tool could not retrieve data (cap hit, file not found,
         out-of-scope capability): write `UNKNOWN — <one-line reason>` for
         that sub-question.
       - Keep process narration out of the recorded answer.

  5. bash benchmark/rtk/scripts/record.sh <n> "<your-model-id>" /tmp/answer.md
     Leave `--allow-zero` unused for benchmark runs.
     If zero rows are reported, reconfigure and redo the question.

  6. Move to n+1.

═══════════════════════════════════════════════════════════════════
FINALISE — after the last question is recorded
═══════════════════════════════════════════════════════════════════

node benchmark/rtk/scripts/finalize.mjs "$RUN"
  # writes $RUN/output.md and $RUN/summary.json

═══════════════════════════════════════════════════════════════════
VALIDITY CHECKLIST
═══════════════════════════════════════════════════════════════════

• Run set-q.sh BEFORE the first tool call for each question.
• Sequential: finish Q<n> (including record.sh) before Q<n+1>.
• Use the assigned toolset for the run. Mixing tools makes the measurement unusable.
• Leave `--allow-zero` unused. A zero-row question is a metering failure.
• If a sub-question is unanswerable due to tool limitations (e.g. rtk
  researcher cannot look up npm registry, cannot read remote directory
  without cloning), write UNKNOWN — <reason>.
• Keep the run blind until finalized: leave the other agent's output and
  output/summary.md.
```
