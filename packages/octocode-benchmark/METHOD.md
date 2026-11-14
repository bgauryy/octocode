# Method

How a benchmark round is run, measured, judged and recorded. Agents run it by direct calls and
judgement: there are no driver scripts. A one-line count (`| wc -c`, `time`) is fine; don't keep
it. What to compare against is in [COMPARISONS.md](COMPARISONS.md); how to check each tool's
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
| **Provider tokens and cost** | host-reported, when available | report beside characters; unknown stays unknown, never zero |

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
   [v18 → v20 tool map](results/baselines/release-notes-v18-v20.json). A workflow must not need more calls
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
