# Benchmark harness

Runs a research round from one round manifest, grades it blind, turns the stored transcripts into numbers, and
checks that a leaner build still returns the same evidence. The method it follows is in [METHOD.md](../METHOD.md);
the environment each session gets is in [TESTING_ENV.md](../TESTING_ENV.md).

```bash
DEV='node skills-dev/octocode-dev/scripts/dev.mjs'
ROUND=<round manifest>.json
$DEV bench run     $ROUND [--pass N] [--only gh-Q1,gh-Q2] [--arms v20-cand,gh] [--dry-run]
$DEV bench judge   $ROUND first        # then: $DEV bench judge $ROUND swap
$DEV bench analyze $ROUND
$DEV bench replay  $ROUND --base base --cand cand [--limit N] [--declare changes.json]
$DEV bench test                        # the harness's own tests
```

`run` and `judge` start paid model sessions; `analyze` and `replay` start none. Every command takes `--help`.

| Command | Reads | Writes | Fails when |
|---|---|---|---|
| `run` ([run.mjs](run.mjs)) | the manifest, the question files | `campaigns/<round>/pass-<n>/runs/<set>-<id>-<arm>/`: prompt, transcript, answer, result; `run.log` | preflight fails (exit 2), or a session returns no result (exit 1) |
| `judge` ([judge.mjs](judge.mjs)) | stored answers, the [judge brief](../briefs/judge.md), answer keys | `campaigns/<round>/pass-<n>/judge/<set>-<id>[-swap]/`: prompt, order, judgement, verdict | a judge returns no complete verdict |
| `analyze` ([analyze.mjs](analyze.mjs)) | stored runs and verdicts | `campaigns/<round>/analysis/`: `numbers.json` (schema version 2) and `tables.md` | no run is stored |
| `replay` ([replay.mjs](replay.mjs)) | stored transcripts, two pinned binaries | `campaigns/<round>/replay/<base sha>-<cand sha>.json` | the candidate loses evidence the base returns (exit 1) |

Stored answers and verdicts are never overwritten: a run or a grading that already has its result is skipped.
Scratch (session homes, MCP configs, the `gh` wrapper's PATH folder, the replay home) lives under
`.octocode/tmp/<round>/` and is removed when each run ends, read-only Go module caches included.

## Round manifest

A JSON file you write for the round; paths in it are relative to the repository root, or start with `~`.

| Field | Meaning |
|---|---|
| `round` | The round's name: the campaign folder and the scratch folder are named after it |
| `passes`, `jobs` | How many times every question is answered, and how many sessions run at once |
| `passDir` | Where a pass is stored inside the campaign; `pass-{n}` unless stated |
| `seed` | Fixes the judge's answer order and the bootstrap interval; the round name unless stated |
| `neutralCwd` | The empty directory sessions start in. It must sit outside any tree that has `CLAUDE.md` or `AGENTS.md` above it |
| `clones` | The pinned local clones: served to Octocode arms, readable by the judge |
| `models` | `runner` and `judge` model IDs |
| `timeouts` | `runSeconds` and `judgeSeconds`: when a session is ended and recorded as failed |
| `pins` | Named binaries, each with `path` and `sha256`. A pin that is not the recorded file stops the round |
| `arms` | Named toolsets. `kind: "mcp"` gives `server` (`command`, `args`, `env`) and optionally `pin`; `kind: "shell"` gives Bash with `gh` and the listed `filters`. Each has the `instruction` line added to the question |
| `sets` | Question sets: `questions` (folder), `ids` (`"Q1-Q30"` or a list), `arms` (per arm `"all"` or the IDs it answers) |
| `groups` | Named groups of questions for the tables; one per set unless stated |
| `compare` | `[arm, otherArm]` pairs for the paired ratios |
| `judge` | Optional: `arms` to grade, `brief` to use |

Placeholders in an arm's `server`: `{repo}`, `{bench}`, `{clones}`, `{scratch}`, `{home}` (the session's own
Octocode home), `{pin}` (the arm's pin) and `{pin:<name>}`.

**Two builds side by side.** Give each build its own arm and pin, for example `v20-base` and `v20-cand`. Both
start the same server with `NODE_ENV=development` and `OCTOCODE_NATIVE_BIN` set to `{pin}`. Freeze each binary
first with `$DEV pin` and copy the path and SHA-256 it prints.

## Run

`run` checks before the first session: every pin's SHA-256, every question file, the neutral directory, the host
and `gh` versions, a GitHub token from `gh auth token`, and each server's files. Then it starts one fresh session
per pass, question and arm, in this order:

- pass 1 of every question and arm before any run of pass 2;
- inside a pass, the arms of one question next to each other, so they share the same minutes;
- the arm that goes first rotates with the question and the pass.

Each session gets a cleaned environment (no Octocode, GitHub or host variables from the caller), no project
settings, and only its arm's tools. A shell arm finds [gh-readonly](gh-readonly) first on its PATH: it refuses
writes and non-GET API calls, passes everything else to the real `gh` unchanged, and logs each call to
`gh-log.jsonl` in the pass folder.

## Judge

`judge first` gives each question to one fresh judge: answers shown as X, Y, Z in a seeded random order, with
the product name, tool names and the clone path removed. The judge gets the clones through `--add-dir` and the
read-only `gh`. `judge swap` grades again, in reverse order, every question whose first scores differ by 2 or
more points; [analyze](#analyze) counts a win only when both gradings agree.

**Fact-level answer key.** When `questions/<set>/answers/<id>.md` exists, the judge gets it as a rubric to verify.
When the key also lists facts, the judge scores each fact for each answer as `right`, `missing`, `wrong` or
`unsupported`, beside the 0–10 score. A key lists facts as the rows of a table that has a `Fact` column, or as
list items under a `Facts` heading. With a `#` column only the numbered rows are facts; a `Not the answer` row
names a distractor and is not scored:

```markdown
| # | Fact | Value | Line |
|---|---|---|---|
| 1 | Defining file | `route-regex.ts`, `export function getRouteRegex(` | 149 |
| 2 | Helper it calls | `getParametrizedRoute(…)` | 157-161 |
| Not the answer | `routeKeys` | returned by `getNamedRouteRegex` | 378 |
```

A verdict is stored only when it scores every answer and states every fact of every answer.

## Analyze

Prints per group of questions and writes the same to `numbers.json` and `tables.md`:

- **Per arm:** answers, quality, tool calls and failed calls, model calls, reply characters, tokens and cost.
- **Paired ratios** for each `compare` pair: per-question ratio of cost, new input, cached re-reads, output,
  model calls, tool calls and reply characters, as a geometric mean with a 95% bootstrap interval (questions
  resampled 10,000 times, seeded), the median, the range, and on how many questions each side is lower. With
  several passes: each pass's ratio and the questions whose cost ratio changes side.
- **Ledger per arm:** reply characters and tokens by tool and by output field path (array steps as `*`); for a
  shell arm by `gh` command and by the filters it was piped through; file requests by mode (whole, ranges,
  filter, outline); cursor calls; failed calls by cause, refusals apart from errors; first-call cached tokens.
- **Price of one more model call:** the cached re-read of the context so far plus its output, and beside it the
  new input an average call reads. Prices per token are fitted from the round's own usage and reported cost.
- **Facts**, when the verdicts carry them: right, missing, wrong and unsupported per arm, and cost per answer
  with every fact right.

Tokens per reply are the provider's, not a tokenizer's: the new input a model request reports is shared between
the replies it read and the model's own previous turn by character count. Per arm the attributed tokens add up
to the provider's new-input total.

## Replay

The gate for a change that makes replies smaller. It takes every distinct Octocode call from the round's stored
transcripts and runs it on both binaries through the CLI, following every top-level `next` to the end with
[walk.mjs](../../../skills-dev/octocode-dev/scripts/walk.mjs):

```bash
NODE_ENV=development OCTOCODE_NATIVE_BIN=<pin> node packages/octocode/out/octocode.js <tool> '<json>' --json
```

The rebuilt rows are reduced to evidence items ([evidence.mjs](evidence.mjs)): a file line is path, line number
and text; a search hit is path and line, and its text may gain words but not lose them; a patch is its lines; any
other value counts under its field path. Continuations, hints, paging counters and per-run values are left out.

| Comparison | Passes when |
|---|---|
| Reachable | every item the base returns over all pages is returned by the candidate over all pages |
| First reply | every item in the base's first reply is in the candidate's first reply |
| Contract | the candidate's walk breaks no continuation rule the base's walk kept |

A failed call is printed with its request and the missing items. A change that is meant to move evidence out of
the first reply is listed in a `--declare` file, with a pattern for the items and a reason; declared items are
reported and do not fail. An item the base itself no longer returns on a second run is reported as unstable (a
live provider answer moved). A call that fails on the base has nothing to compare and is counted apart.

Calls that created state other calls read (a clone, a materialized tree) run first; both builds share one fresh
Octocode home, and session paths in stored requests are rewritten to it.

## Files

| File | Holds |
|---|---|
| [bench.mjs](bench.mjs) | the `dev bench` entry |
| [lib.mjs](lib.mjs) | manifest loading and checks, paths, seeded generator, job pool, transcript reader, cleanup |
| [testing.mjs](testing.mjs) and `*.test.mjs` | the tests: `$DEV bench test` |
