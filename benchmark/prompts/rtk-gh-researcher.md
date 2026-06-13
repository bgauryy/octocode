# RTK + gh Researcher Prompt

Paste this prompt to the agent assigned `researcher: rtk-gh`.

```
ROLE
You are the RTK + gh benchmark researcher. Answer every question in:

  benchmark/questions/nextjs.md

Write your measured run to:

  benchmark/output/rtk-gh/

Your job is research only. Do not judge, compare, or read another agent's
answers. Do not read benchmark/output/octocode/ or benchmark/output/summary.md
before your run is finalized.

MANDATORY TOOLING RULE
Every RTK command must go through:

  bash benchmark/scripts/rtk-meas.sh <rtk-subcommand-and-args>

Every bare GitHub CLI command must go through:

  bash benchmark/scripts/gh-meas.sh <gh-subcommand-and-flags>

Bare rtk or gh commands are unmetered and invalidate the question.

METERING
The wrappers log `in_chars` and `out_chars`; their sum is the canonical
cost measurement. Approx tokens are computed later as `ceil(chars / 4)`
for display only. Do not claim the run measures true tokenizer tokens
unless `lm_tokens_in` and `lm_tokens_out` are explicitly recorded.

SETUP
From the repository root:

  rm -rf /tmp/nextjs-bench
  git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench
  rm -rf benchmark/output/rtk-gh
  source benchmark/scripts/init-run.sh rtk-gh

CALL FORMAT
Use RTK for local clone research:

  bash benchmark/scripts/rtk-meas.sh rg 'notFound' /tmp/nextjs-bench/packages/next/src
  bash benchmark/scripts/rtk-meas.sh read /tmp/nextjs-bench/packages/next/src/server/base-server.ts
  bash benchmark/scripts/rtk-meas.sh ls /tmp/nextjs-bench/packages/next/src/server
  bash benchmark/scripts/rtk-meas.sh find /tmp/nextjs-bench/packages/next/src --name '*.ts'

Use gh for GitHub API and PR research:

  bash benchmark/scripts/gh-meas.sh api repos/vercel/next.js/contents/packages/next/src/server
  bash benchmark/scripts/gh-meas.sh search code 'notFound repo:vercel/next.js' --json repository,path,textMatches
  bash benchmark/scripts/gh-meas.sh api repos/vercel/next.js/contents/.github/workflows
  bash benchmark/scripts/gh-meas.sh api repos/vercel/next-evals-oss/contents
  bash benchmark/scripts/gh-meas.sh search repos 'next evals org:vercel' --json fullName,description

Keep each command focused on evidence needed for the current question. Avoid
broad dumps or repeated searches after you already have enough source evidence.

PER-QUESTION LOOP
For n = 1 to `cat "$RUN/.q-count"`:

  1. bash benchmark/scripts/set-q.sh <n>
  2. Read only Q<n> from benchmark/questions/nextjs.md.
  3. Research through benchmark/scripts/rtk-meas.sh or benchmark/scripts/gh-meas.sh only.
  4. Write /tmp/answer.md.
  5. bash benchmark/scripts/record.sh <n> "<model-id>" /tmp/answer.md

Answer format:

  - Start directly with bullets; no "## Answer" header.
  - Include exact file paths, line numbers, PR numbers, identifiers, and quotes.
  - Keep command transcripts and process notes out of the recorded answer.
  - If a fact cannot be found after real research, write:
    UNKNOWN - <one-line reason>

FINALIZE

  node benchmark/scripts/finalize.mjs "$RUN"

VALIDITY RULES

  - Run set-q.sh before the first metered command for each question.
  - Use rtk-meas.sh or gh-meas.sh for every research command.
  - Finish and record Q<n> before starting Q<n+1>.
  - Do not use record.sh --allow-zero.
  - Stay blind to other runs and judge outputs.
```
