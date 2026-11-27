# Method

How a benchmark round is run, measured, judged and recorded. Agents run it by direct calls and
judgement; checked-in scripts do the exact, repeated parts (`dev pin`, `dev walk`, `harness-tally.mjs`), and
a research round is driven by the [harness](harness/README.md) (`dev bench`). What to compare against is in [COMPARISONS.md](COMPARISONS.md); how to check each tool's
behavior, security and skills is in [CHECKS.md](CHECKS.md). Every round ends in one
[scorecard](briefs/SCORECARD.md).

## Kinds of round

| Round | Unit | Who answers | Judged by | Brief |
|---|---|---|---|---|
| **Research round** | one question from a [question set](questions/README.md), per arm | a fresh, isolated agent per question and arm | a blind judge per question | [runner](briefs/runner.md) + one [arm brief](briefs/arms/) + [judge](briefs/judge.md) |
| **Capability round** | one task per tool row (e.g. "a function's body over 400 lines") | one checking agent, both sides | the checking agent, by reading | [check-local](briefs/check-local.md), [check-github](briefs/check-github.md), [local flows](local/FLOWS.md) |
| **Readiness round** | one version: every area of [CHECKS.md](CHECKS.md) | checking agents per area | by reading, against the pass criteria | [CHECKS.md](CHECKS.md) + [scorecard](briefs/SCORECARD.md) |

A research round answers "does an agent reach a correct answer with less context?". A capability
round answers "for this job, is Octocode's output better than the expert's command?". Use a
research round for a release headline, a capability round after a tool change, and a readiness round
before a release; a release round usually runs all three and records them in one scorecard.

## Arms

An arm is the toolset an agent gets. The arms are Octocode (CLI or MCP, one pinned build), plain
gh, gh-lean, gh + RTK, gh + Headroom and the lean local shell; their briefs are in
[briefs/arms/](briefs/arms/).

- **Reuse or refresh deliberately.** Stored gh, gh-lean, RTK, Headroom and old-version answers
  remain useful historical baselines (see the [results index](results/README.md)). Rerun the
  relevant competitors when the user requests a fresh comparison, inputs/provider behavior have
  changed, or a current speed/cache claim needs paired observations. Label historical and fresh
  evidence separately. Do not pool different questions, representations or cache states.
- **What is stored where.** gh-lean answers are stored in full, so a new Octocode answer can be
  judged blind against them. Plain gh, RTK and Headroom survive only as per-question numbers
  from August 2026 (characters and correctness from another judge and date): compare against
  them as numbers and say so.
- **Pin the build.** One binary per Octocode arm: record its version and SHA-256, and don't let a
  rebuild replace it mid-round. Time release builds only; a development build is several times
  slower.
- **Preflight.** Before the first question: `--version` of every arm's tool, `gh auth status`,
  and one real call per arm that returns an answer. A failed preflight invalidates the round.

## Isolation and fairness

Setting up the agents, tool allowlists, Octocode home, clones and cleanup is in [TESTING_ENV.md](TESTING_ENV.md).

- **Research rounds: one fresh agent per question and arm**, with only its brief, its arm and
  the question. No answer key, other arm's answer, project instructions, skills or memory. Seeking
  a hidden reference invalidates that research run. Capability rounds may use one checker with
  frozen questions and separate source oracles; label those as direct checks, not blind agent trials.
- **Same question, same frozen refs, same model and budget** for every arm in a comparison; only
  the tools differ. Arms run on different days or with different models are labelled as such.
- **Leanest legitimate path for every arm.** Each brief asks for it. A whole-tree or whole-file
  dump where a targeted read answers inflates that arm: mention it next to the number.
- **Format asymmetry.** Octocode returns shaped output; gh returns raw output unless the arm
  projects it (gh-lean) or compresses it (RTK, Headroom). That difference is the product, but
  state it beside the ratio.
- **Protect source and upstream state.** Research arms do not edit the source repositories or
  write to GitHub. Capability questions may create owned disposable fixtures, checkouts and
  caches, and make the explicit edits needed to test freshness or dirty-checkout protection.
  Freeze originals, record each mutation, restore them and clean up only owned state. An
  unplanned source/upstream write invalidates the run; do not claim permission enforcement
  unless the host actually supplies it.
- **Rate limits.** Serialize paired timing windows. Outside them, honor provider limits and
  record throttling, retry cost and shared-token contention; do not infer a universal safe
  concurrency count.
- **Live GitHub targets.** Public repositories; never `facebook/react` (renamed, and some
  repositories are refused by the API). Confirm a GitHub defect on a second repository.
- **Overfitting.** The questions are public and Octocode is developed against them. Before a
  round, search the briefs and the live tool instructions (`octocode schema`, the MCP
  instructions) for each question's distinctive terms (repository names, paths, identifiers)
  and remove any hit. A product rule written after a question failed makes that question a weak
  test: confirm headlines on questions written after the change.

## What to measure

| Measure | How | Notes |
|---|---|---|
| **Correctness** 0–10 | blind judge, from the source | the gate: cost counts only when correctness holds |
| **Depth** 1–5 | blind judge | evidence behind each claim; length earns nothing |
| **Characters** | text that reached the model: every tool result read, every call written, and the final answer, in Unicode code points | Count from receipts/transcripts with a Unicode-aware counter. `wc -c` measures bytes, so record UTF-8 bytes separately. Character/token ratios vary; token cost stays unknown without provider usage. Capture both CLI streams. Setup/help/schema and failed calls remain separately visible; do not charge shared environment bookkeeping on every logical request or claim unread captured help was model context |
| **Calls** and **failed calls** | from the transcript | name the cause of each failure (a guessed field name, a refused command) |
| **Time** | wall clock per question; Octocode first and repeat call separately; warm (session server running) and cold | compare only arms timed in the same round on the same machine |
| **Provider tokens and cost** | host-reported, when available | report beside characters; unknown stays unknown, never zero. Split new input (uncached plus cache writes), cached re-reads and output. A run within the cache lifetime of a run with the same prompts re-reads that run's question text and early turns: its first call writes nothing and its cost is understated (2026-11-24: $1.56 against $1.93 on a fresh prompt). Check each session's first-call cache write before comparing cost across runs |

### Workflow receipts

Record one row per [workflow](CHECKS.md#workflows-each-must-finish-in-the-same-number-of-calls-as-last-round),
including blocked rows. Give the fixed scope, surface and build, attempted and failed calls, input and output
UTF-8 bytes, elapsed time, provider requests where applicable, and the source or invariant that verifies
completion. Count required continuations and retries as calls. Keep preflight, independent verification
and optional exploration outside the workflow totals. Name every required step that could not run.

Use compact JSON arguments for input bytes. Count both CLI output streams or the MCP result payloads,
and state which was measured. Label summed call times separately from end-to-end wall time. For a repeat
sample, run identical inputs consecutively on the same surface and compare the deciding evidence. State
what was reset, what persisted, whether startup is timed, and which workflows had no repeat. A shared
service with uncontrolled caches yields first-observed and repeat timings. A controlled cold session
requires a fresh process and declared cache state. Compare latency and call-count parity only with
matched cases, release builds for timings, and comparable measurement boundaries. Store the compact table
as `EFFICIENCY.md` alongside the round scorecard.

## Judge

The [judge brief](briefs/judge.md) is the whole procedure. In short:

- answers are shown as X and Y in a random order per question, with tool names and commands
  removed; the mapping stays private until grading is sealed;
- the judge establishes the facts from the source at the commit the answers cite, before
  reading them;
- correctness is scored first; costs stay hidden from the judge;
- a gap of 2 or more points is judged again by a fresh judge with the order swapped; a win
  counts only if it survives, otherwise the question is marked unresolved and left out of the
  tally (state the count);
- use a judge from a different model family than the runners where possible, and record it.

## Aggregate

- **The question is the unit**, and arms are paired on it.
- **Characters:** per question, divide one arm's characters by the other's. The headline is the
  geometric mean of those ratios with a 95% bootstrap interval (resample questions), plus the
  median, the range and on how many questions each side was leaner. A pooled total is dominated
  by a few huge questions: show it only as a budget, with the top question's share and the
  result without it.
- **Correctness:** paired win, tie and loss counts. Scores within one point count as tied, since
  the same answer can move a point between judgings. Mean gaps near the ceiling are noise.
- **Decision:** an arm that is net less correct cannot win on characters. When correctness is
  indistinguishable, characters and time decide.
- **Variance:** one pass shows a direction. A size claim needs three passes; report each pass's
  ratio and the spread, and flag questions whose ratio changes sign between passes.
- **Invalid and missing runs** stay in coverage counts and out of paired ratios.

## Accept a cost claim

A claim that one arm costs less than another per answer ("v20 costs no more than `gh`"), and any change shipped
to make replies smaller, is accepted only when all of these hold. `dev bench analyze` and `dev bench replay`
([harness](harness/README.md)) print each number.

| Gate | Rule | Printed by |
|---|---|---|
| Passes | Three passes, interleaved: every arm answers pass 1 of every question before any arm starts pass 2, and the arms of one question run next to each other | `bench run` order |
| Cost ratio | Per question, the claimed arm's cost divided by the other's, summed over passes. The 95% bootstrap upper bound of the geometric mean is at or below 1.00, and each pass's geometric mean is at or below 1.00 | `bench analyze`, cost ratio and cost ratio per pass |
| Where the cost is | New input, cached re-reads, output and model calls are reported as the same paired ratios, with how many questions each side is lower on. They explain the cost ratio and gate nothing | `bench analyze` |
| Facts | With a fact-level answer key, the claimed arm has no fact scored missing, wrong or unsupported that the other arm, or the same questions on the previous build, has right. The 0–10 score cannot show this near its ceiling | `bench judge`, then the facts line of `bench analyze` |
| Evidence | For a new build: replay of the round's stored calls against the previous pinned build loses no evidence, reachable or in the first reply, beyond the changes declared with a reason | `bench replay --base <previous> --cand <new>` exits 0 |
| Cache state | Every session's first call reads the prompt from cache, or none does. Cold and warm sessions are never pooled | first-call cached tokens and the cold count in the ledger |

Verdicts: **Accept** when every gate holds. **Inconclusive** when the interval straddles 1.00, when fewer than
three passes ran, or when a question's cost ratio changes side between passes and removing it moves the bound
across 1.00. **Reject** when the lower bound is above 1.00. **Regression** when a fact or a replay item is lost,
whatever the cost.

Report the price of one more model call from the same round beside any trade of reply size for calls: a smaller
first reply that needs a second request pays the cached re-read of the whole context and the output of that
request (25 November 2026, GitHub set: $0.012 for v20, $0.009 for `gh`).

## Compare a new version

Every version is compared twice: against the previous version, and against the stored baselines. Both
comparisons use the same questions and the same per-question pairing.

1. **Identify the version.** Record `octocode --version`, the native binary's SHA-256, the commit (and
   whether the tree was dirty) and the date. A round without these cannot enter the timeline.
2. **Against the previous version.** Find it in the [version timeline](results/README.md#version-timeline).
   - For a research round, run the new build on the same question IDs, then judge blind against the previous
     version's stored answers when they exist (v20 and gh-lean on GitHub 30 do). Otherwise use the
     per-question numbers in [results/baselines](results/baselines/), and say the comparison is numeric
     only, with a different judge and date.
   - For speed, time both versions on the same machine, interleaved: the previous one through
     `npx -y octocode@<previous>`, warm and cold, release builds only.
   - For a capability round, repeat the previous scorecard's tasks with the same targets and compare row by
     row: rating, characters, calls and time.
3. **Against the stored baselines.** Use the per-question tables in [results/baselines](results/baselines/):
   gh-lean blind (its answers are stored), plain gh, RTK and Headroom as numbers, the lean shell for local.
   Refresh the relevant arms when required by the current question or the user's request, and
   keep the refreshed measurements distinct from stored numbers.
4. **Per tool across versions.** Match tools by name; where a tool was renamed, use the
   [v18 → v20 tool map](results/baselines/release-notes-v18-v20.md). A workflow must not need more calls
   than the previous round recorded.
5. **Decide.** Better, same or worse per comparison, with the verdicts below. A new version that is less
   correct is a Regression whatever its characters.

## Record

Every round writes `results/rounds/<version>-<YYYY-MM-DD>/` (tracked) and adds a row to the
[version timeline](results/README.md#version-timeline):

- `SCORECARD.md`, filled in from the [template](briefs/SCORECARD.md): the verdict per area first, then the
  ratings, gates, benchmark deltas, defects and what was not checked;
- `README.md` for a research or capability round: the round identity (version, binary SHA-256, commit,
  date, question set and IDs, arms, runner and judge models, the briefs used), the per-question table and
  aggregate, failures and invalid runs with their causes; in a capability round, the row table from the
  brief and every Octocode defect with the exact call that shows it.

Answers, verdicts with their reasoning, and raw transcripts are large: put them in
`campaigns/<version>-<YYYY-MM-DD>/` (local, gitignored) and name that folder in the round's README. When the
round becomes a baseline for later versions, add its per-question numbers to
[results/baselines](results/baselines/). Never overwrite stored answers or verdicts, never rerun a valid
answer because it scored poorly, and never put credentials or private rubrics in a report.

What the verdicts mean:

- **Accept:** the evidence supports the claim.
- **Reject:** the evidence contradicts the claim.
- **Regression:** the new build is measurably worse than the previous one.
- **Inconclusive:** the direction holds, but the size needs a cleaner run.
- **Invalid:** the comparison was not fair or not measured; no claim.

For per-tool evaluations, use the [specific questions](questions/tools/README.md), shared
[CLI/MCP checks](CHECKS.md#every-tool-every-time) and [1–10 rubric](RATINGS.md). Every tool gets its
own row, including graphBuild and graphQuery. A provisional rating must name the unverified
dimension; historical ratings are never a substitute for a new execution result.
