# Running a matchup with isolated agents

Load when actually executing a benchmark. This is the concrete orchestration recipe
for the by-hand flow whose design lives in `../../BENCHMARK.md`. Paths here are
relative to the benchmark package root (`packages/octocode-benchmark/`).

Non-negotiable isolation (see `SKILL.md` → "Operate it correctly"): a fresh agent
per (question, arm), one judge agent per question, no shared transcripts, no answer key.

## 0. Preflight — prove tools and scripts work before any question

Run once per matchup and record versions in the write-up. A failed preflight invalidates the run.

| Arm | Confirm | Command |
|---|---|---|
| gh (all `gh*` arms) | installed + authed | `gh --version` · `gh auth status` |
| `gh` + RTK | rtk present | `rtk --version` and one probe `rtk gh search repos <x> --limit 1` |
| `gh` + Headroom | wrapper runs + compresses | `./compare/octocode-vs-gh-headroom/bin/ghc api rate_limit` — a `0%` ratio or `router:protected` means compression is OFF; fix before running (measurement is invalid otherwise) |
| Octocode (arm B) | CLI + a live tool call | `npx octocode --version` and a probe `npx octocode tools ghSearchRepos --queries '{"keywords":["is"],"owner":"sindresorhus","limit":1}'` |

Also read the matchup's `compare/<matchup>/README.md` for its exact allowed (read-only) surface, and confirm the three arm primers exist in `../../RUNNER_TOOL_CONTEXT.md` (`## Octocode arm`, `## gh + RTK arm`, `## gh + Headroom arm`).

## 1. Measurement wrapper (transparent — required)

Every research command runs through a wrapper that prints the real CLI output, counts
its Unicode characters, and appends one JSONL row. The design explicitly allows a
transparent wrapper whose child process is exactly the research command; it must not
alter the request or response. Never trust a runner's self-reported char count.

```bash
# .octocode/tmp/measure.sh  — usage: measure.sh <ARM> <QID> <LABEL> -- <command...>
set -uo pipefail
ARM="$1"; QID="$2"; LABEL="$3"; shift 3; [ "$1" = "--" ] && shift
RUNDIR="$(cat "$(dirname "$0")/CURRENT_RUN")"; OUT="$RUNDIR/$ARM"; mkdir -p "$OUT"
CMD="$*"; T0=$(perl -MTime::HiRes=time -e 'printf "%.0f",time()*1000')
O="$("$@" 2>&1)"; RC=$?; T1=$(perl -MTime::HiRes=time -e 'printf "%.0f",time()*1000')
C=$(printf '%s' "$O" | perl -CS -ne '$c+=length; END{print $c}')
{ echo "### [$QID/$LABEL] rc=$RC chars=$C ms=$((T1-T0))"; echo "\$ $CMD"; echo "$O"; } >> "$OUT/$QID.out.txt"
python3 -c 'import json,sys;open(sys.argv[1],"a").write(json.dumps({"arm":sys.argv[2],"qid":sys.argv[3],"label":sys.argv[4],"chars":int(sys.argv[5]),"ms":int(sys.argv[6]),"rc":int(sys.argv[7]),"cmd":sys.argv[8]})+"\n")' "$OUT/calls.jsonl" "$ARM" "$QID" "$LABEL" "$C" "$((T1-T0))" "$RC" "$CMD"
printf '%s\n' "$O"
```

Setup once per run:
```bash
RUNDIR=".octocode/tmp/run-$(date +%H%M%S)-$(date +%Y-%m-%d)"
mkdir -p "$RUNDIR/A" "$RUNDIR/B"; echo "$RUNDIR" > .octocode/tmp/CURRENT_RUN
chmod +x .octocode/tmp/measure.sh
```

For an instrumented arm (`rtk`/`headroom`), read chars from that arm's own log if it
is authoritative; otherwise the wrapper's count of what the wrapper emitted is the figure.

## 2. Spawn topology — who gets what

Spawn every agent with only the tools it needs (a runner needs `bash` to run its CLI;
the judge needs independent research tools). Give each **only its own inputs** — never
the competing arm's primer, another agent's transcript, or any answer key.

### Runner agent (one per question per arm)
Inputs: `../../RUNNER.md` + this arm's section from `../../RUNNER_TOOL_CONTEXT.md` + the
question file (`compare/github-questions/Q<n>.md` or the matchup's `questions/Q<n>.md`).
Task: freeze mutable refs (branch/PR-state/SHA + UTC) first; research the leanest legal
path on the assigned surface only; run **every** command through the wrapper
(`measure.sh <A|B> Q<n> <label> -- <cmd>`); append an answer block (Answer + Research steps).

Arm B research command form: `npx octocode tools <tool> --queries '<json>'` only — no MCP, no gh.

### Judge agent (one per question)
Inputs: the question + the two finished answers labelled **X** and **Y** with tool names
hidden + its own research tools (e.g. `npx octocode tools …` or `gh`). Task per
`../../JUDGING.md`: establish ground truth by its own current-evidence research
(structured facts need an exact unminified read), score X then Y independently per
`../../SCORING.md` (correctness 0-10, depth 1-5, workflow 1-5, chars in/out), then compare.
FORBID it from trying to recover which tool produced X or Y.

### Scaling without breaking isolation
Spawning one process per (question × arm × 20) is heavy. You MAY batch several questions
into one runner agent to cut process count — but the two arms MUST stay in separate agents,
and the judge MUST never share an agent/transcript with either runner. Example that keeps
isolation: `A:Q1-10`, `A:Q11-20`, `B:Q1-10`, `B:Q11-20` (4 runner agents), then judge
per question (or a judge agent that never saw runner transcripts, receiving only X/Y).

## 3. Outputs and how to use them

| Artifact | Location | Use |
|---|---|---|
| Per-call measurements | `<RUNDIR>/<A\|B>/calls.jsonl` | authoritative char/call source; sum per question and total |
| Raw preserved output | `<RUNDIR>/<A\|B>/Q<n>.out.txt` | evidence trail; re-derive counts if disputed |
| Runner answers | `<RUNDIR>/<A\|B>/ans_*.md` | inputs the judge grades (as X/Y) |
| Final write-up | `results/<matchup>-<HHMMSS>-<YYYY-MM-DD>.md` | per `../../REPORT_TEMPLATE.md`: per-question table + summary |
| Rollup | `results/SUMMARY.md` + `results/README.md` | update the matchup's headline row to the new report |

Verify before concluding: recompute totals/means from `calls.jsonl` and confirm the
report table matches row-for-row; confirm every headline link resolves.

Decide correctness-first (`../../SCORING.md`): B clearly higher correctness → B wins;
essentially equal → tie broken by fewer delivered characters; a confidently-wrong answer
cannot win regardless of footprint. One pass is a snapshot — repeat for a stable claim.
