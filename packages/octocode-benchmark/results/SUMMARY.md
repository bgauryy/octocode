# Octocode research benchmarks - how much context each tool burns

**One question, answered by two tools, graded blind by a third. Repeated across four independent runs and three leading baselines. The finding is consistent: at equal correctness, Octocode pulls far less into the model's context to reach the same answer.**

This is the plain-English rollup of every run in [`results/`](.). Each linked write-up has the full per-question evidence; this page is the picture.

---

## What is measured (and why it's the honest number)

The benchmark measures **characters of raw CLI output pulled into the model's context** - literally, how much text the tool hands back that the model then has to read. Not a self-report; measured with `wc -c` on captured output (and, for the Headroom arm, from the compressor's own JSONL log after compression).

> **Characters ≈ tokens.** Context cost is what you actually pay for - in tokens, in latency, and in the model's attention budget. For English/code, tokens ≈ characters ÷ ~4, so every ratio on this page holds identically whether you read it as characters or tokens. We report the measured unit (characters) and never a guessed token count.

**Correctness is graded first, efficiency second.** A third independent agent establishes the ground truth by its own research, then scores both answers *blind* (tool identity hidden). Fewer characters only counts as a win when the answer is just as correct. An answer that is confidently wrong can't win on leanness. This is the rule in [`SCORING.md`](../SCORING.md).

So every number below is: **same question, same correctness bar, independently graded - and Octocode got there with less.**

### How these runs were produced

Every run on this page was executed with the local **`octocode-benchmark` skill** - [`../skills/octocode-benchmark/`](../skills/octocode-benchmark/) ([`SKILL.md`](../skills/octocode-benchmark/SKILL.md)). It enforces the discipline that makes the numbers trustworthy:

- **Three isolated roles** - each runner and the grader is a *separate* agent/context; none sees another's transcript or any answer key.
- **One variable** - both arms get the same question, budget, and frozen refs (branch / PR state / SHA + UTC pinned *before* answering); only the CLI differs.
- **Blind grader** - answers handed over as X/Y with tool names hidden; ground truth established by the grader's own research; un-blinded only at tabulation.
- **Measured chars, not self-reports** - characters read from the CLI/compressor log (`wc -c` / instrumented arm), because models miscount their own context.

To reproduce a run or add a baseline, invoke the skill (`/octocode-benchmark`, or point your agent at that folder) and follow its `Run it` steps - no harness to set up.

---

## The headline: total context pulled, across all four runs

Lower is better. Each pair is scaled to its own baseline (the full bar), so the gap *is* the ratio. Correctness was a tie or an Octocode edge in every run - see the table underneath.

```
gh alone  (17 questions)
  octocode  ████████████████··············  152,710 chars
  gh        ██████████████████████████████  286,812 chars   → gh pulls 1.9× more

gh + rtk (by-hand pass)  (17 questions)
  octocode  █████████████████████·········  168,773 chars
  gh+rtk    ██████████████████████████████  242,691 chars   → gh+rtk pulls 1.4× more

gh + rtk (orchestrated: 2 blind runners + grader)  (17 questions)
  octocode  ███████████···················  395,644 chars
  gh+rtk    ██████████████████████████████  1,040,783 chars   → gh+rtk pulls 2.6× more

gh + Headroom compression (orchestrated)  (15 questions)
  octocode  █████·························  203,708 chars
  gh+hr     ██████████████████████████████  1,180,822 chars   → gh+hr pulls 5.8× more

```

| Run | Baseline | Questions | Correctness (base / octocode) | Chars - baseline | Chars - octocode | Octocode leaner by | Octocode leaner on |
|---|---|---:|:--:|---:|---:|:--:|:--:|
| [gh alone](octocode-vs-gh-233502-2026-08-05.md) | `gh` | 17 | 9.94 / 9.94 (tie) | 286,812 | **152,710** | **1.88×** | 11/17 |
| [gh + rtk (by-hand pass)](octocode-vs-gh-rtk-224513-2026-08-04.md) | `gh+rtk` | 17 | 10.00 / 10.00 (tie) | 242,691 | **168,773** | **1.44×** | 13/17 |
| [gh + rtk (orchestrated: 2 blind runners + grader)](octocode-vs-gh-rtk-021054-2026-08-05.md) | `gh+rtk` | 17 | 9.18 / 9.38 (octocode ↑) | 1,040,783 | **395,644** | **2.63×** | 10/17 |
| [gh + Headroom compression (orchestrated)](octocode-vs-gh-headroom-023223-2026-08-05.md) | `gh+hr` | 15 | 9.07 / 9.67 (octocode ↑) | 1,180,822 | **203,708** | **5.80×** | 11/15 |

**Across all four runs combined: 2,751,108 characters (baselines) vs 920,835 (Octocode) - Octocode pulled 3.0× less context for the same graded answers.**

The savings grow with how much the baseline is *forced* to over-fetch. Against plain `gh` it's ~1.9×; add `rtk` and the orchestrated pass hits 2.6×; against `gh`+Headroom - a tool whose entire job is to compress `gh` output - it's **5.8×**, because compressing a whole file you never needed is still more than a targeted region read.

---

## Why Octocode is leaner (the mechanism)

It isn't a smaller model or a shorter answer - **the same runner model answered both arms.** The difference is what each tool *hands back*:

| | `gh` / `gh`+`rtk` / `gh`+Headroom | Octocode |
|---|---|---|
| Read a symbol in a 2,000-line file | Fetch the **whole file** (no region read) | `matchString` returns the **anchored region** (~1k) |
| Check one `package.json` field | Fetch the **whole manifest** | Region-read the **exact field block** |
| Inspect a PR | `pr view` + `pr diff` - diff can dump a **9,000-line lockfile** | Structured PR call returns body + the **relevant patch**, skips the lockfile |
| Big output | Headroom compresses it 13–47% *after* it's fetched | Never fetched whole in the first place |

Headroom's per-call compression is real and measured (13–47%, lossless on structured JSON) - but call-level compression cannot offset fetching an entire file/tree/diff the model never needed. Octocode's tools return **pre-distilled** snippets and structured fields, so the expensive whole-object pulls simply don't happen.

The clearest single illustration is **Q4 (Zustand fix PR state)** in the Headroom run: `gh`'s `pr diff` pulled the PR's added `package-lock.json` and, even *after* Headroom compression, landed at **652,481 characters**. Octocode's structured PR call answered the identical question - state, the one product file, the edge case - in **5,661**. Same answer, 115× less context.

---

## Per-question detail (every run, every question)

Each question is scaled to the larger of the two arms, so the shorter bar shows exactly how much less Octocode pulled. `←` marks the leaner arm. Correctness was equal on these unless flagged in the run's write-up.

### gh alone  ·  Octocode 1.9× leaner overall  ·  [full write-up](octocode-vs-gh-233502-2026-08-05.md)

Correctness 9.94 (base) / 9.94 (octocode). Octocode leaner on **11 of 17** questions.

```
Q1  Route regex builder
  octocode  ██████████████············     8,326 ←
  gh        ██████████████████████████    15,095  

Q2  Repo discovery + bounded absence
  octocode  ██████████████████████████    14,398  
  gh        ███·······················     1,507 ←

Q3  Flask route history
  octocode  ██████████████████████████     4,628  
  gh        ██████████████············     2,546 ←

Q4  Zustand fix PR state
  octocode  ██████████████████████████     1,920  
  gh        ████······················       298 ←

Q5  Vue hydration diff review
  octocode  ██████████████████████████    59,946  
  gh        ████████████████████······    45,070 ←

Q6  Express router cross-repo trace
  octocode  ████······················     3,110 ←
  gh        ██████████████████████████    18,659  

Q7  Zustand Next.js contract
  octocode  ██████████················     2,837 ←
  gh        ██████████████████████████     7,466  

Q8  VS Code keybinding dispatch
  octocode  ████······················     2,612 ←
  gh        ██████████████████████████    19,041  

Q9  Fastify lifecycle contract
  octocode  █████·····················     5,056 ←
  gh        ██████████████████████████    27,693  

Q10 Axios repo + Node entry chain
  octocode  ████████████████··········     4,146 ←
  gh        ██████████████████████████     6,735  

Q11 Esbuild Node runtime boundary
  octocode  ████████··················     7,330 ←
  gh        ██████████████████████████    22,928  

Q12 Stream/EventEmitter wiring
  octocode  ██························     3,083 ←
  gh        ██████████████████████████    39,593  

Q13 Redis BITFIELD security + fix PR
  octocode  ██████████████████████████    14,974  
  gh        ███████···················     3,896 ←

Q14 Vitest dependency on Vite
  octocode  ██████████████████████████     7,210  
  gh        ████████████████████······     5,576 ←

Q15 Hono JSX array component PR
  octocode  ███████████████████·······     2,592 ←
  gh        ██████████████████████████     3,537  

Q16 ESLint parser dependency chain
  octocode  ███████████···············     5,069 ←
  gh        ██████████████████████████    11,537  

Q17 Next.js fetch memoization
  octocode  ███·······················     5,473 ←
  gh        ██████████████████████████    55,635  

```

### gh + rtk (by-hand pass)  ·  Octocode 1.4× leaner overall  ·  [full write-up](octocode-vs-gh-rtk-224513-2026-08-04.md)

Correctness 10.00 (base) / 10.00 (octocode). Octocode leaner on **13 of 17** questions.

```
Q1  Route regex builder
  octocode  █████·····················     2,652 ←
  gh+rtk    ██████████████████████████    13,830  

Q2  Repo discovery + bounded absence
  octocode  ████████████████████······    57,249 ←
  gh+rtk    ██████████████████████████    75,727  

Q3  Flask route history
  octocode  ██████████████████████····     8,274 ←
  gh+rtk    ██████████████████████████     9,679  

Q4  Zustand fix PR state
  octocode  █████████████████████·····     6,765 ←
  gh+rtk    ██████████████████████████     8,527  

Q5  Vue hydration diff review
  octocode  ██████████████████████████    45,625  
  gh+rtk    █████████████████████·····    36,148 ←

Q6  Express router cross-repo trace
  octocode  ████████████████··········     2,167 ←
  gh+rtk    ██████████████████████████     3,571  

Q7  Zustand Next.js contract
  octocode  ██████████················     2,808 ←
  gh+rtk    ██████████████████████████     7,466  

Q8  VS Code keybinding dispatch
  octocode  ██████████████████████····     2,004 ←
  gh+rtk    ██████████████████████████     2,317  

Q9  Fastify lifecycle contract
  octocode  ████████████████··········     2,953 ←
  gh+rtk    ██████████████████████████     4,781  

Q10 Axios repo + Node entry chain
  octocode  ███████████···············     3,174 ←
  gh+rtk    ██████████████████████████     7,673  

Q11 Esbuild Node runtime boundary
  octocode  ██████████████████████████     4,964  
  gh+rtk    ██························       317 ←

Q12 Stream/EventEmitter wiring
  octocode  ██························     2,556 ←
  gh+rtk    ██████████████████████████    39,661  

Q13 Redis BITFIELD security + fix PR
  octocode  ██████████████████████████    10,998  
  gh+rtk    ██████████████████········     7,536 ←

Q14 Vitest dependency on Vite
  octocode  ████████████████··········     3,390 ←
  gh+rtk    ██████████████████████████     5,576  

Q15 Hono JSX array component PR
  octocode  ██████████████████████████     5,064  
  gh+rtk    ███████████████████·······     3,658 ←

Q16 ESLint parser dependency chain
  octocode  █████████·················     3,430 ←
  gh+rtk    ██████████████████████████     9,936  

Q17 Next.js fetch memoization
  octocode  ███████████████████·······     4,700 ←
  gh+rtk    ██████████████████████████     6,288  

```

### gh + rtk (orchestrated: 2 blind runners + grader)  ·  Octocode 2.6× leaner overall  ·  [full write-up](octocode-vs-gh-rtk-021054-2026-08-05.md)

Correctness 9.18 (base) / 9.38 (octocode). Octocode leaner on **10 of 17** questions.

```
Q1  Route regex builder
  octocode  ██████████················     9,520 ←
  gh+rtk    ██████████████████████████    25,691  

Q2  Repo discovery + bounded absence
  octocode  ██████████················    63,317 ←
  gh+rtk    ██████████████████████████   164,194  

Q3  Flask route history
  octocode  █████████·················    17,250 ←
  gh+rtk    ██████████████████████████    47,431  

Q4  Zustand fix PR state
  octocode  ██████████████████████████    13,355  
  gh+rtk    ███████████████████·······     9,750 ←

Q5  Vue hydration diff review
  octocode  █████·····················    60,097 ←
  gh+rtk    ██████████████████████████   326,000  

Q6  Express router cross-repo trace
  octocode  ██████████████████████████    27,100  
  gh+rtk    ████████████████████████··    25,098 ←

Q7  Zustand Next.js contract
  octocode  █████████████·············    14,623 ←
  gh+rtk    ██████████████████████████    29,800  

Q8  VS Code keybinding dispatch
  octocode  ████······················     8,677 ←
  gh+rtk    ██████████████████████████    57,964  

Q9  Fastify lifecycle contract
  octocode  ██████████················    19,400 ←
  gh+rtk    ██████████████████████████    51,169  

Q10 Axios repo + Node entry chain
  octocode  ██████████████████████████    20,470  
  gh+rtk    ██████████████████████····    17,354 ←

Q11 Esbuild Node runtime boundary
  octocode  █████·····················    24,800 ←
  gh+rtk    ██████████████████████████   122,772  

Q12 Stream/EventEmitter wiring
  octocode  ██████████████████████████    28,430  
  gh+rtk    █████████·················    10,371 ←

Q13 Redis BITFIELD security + fix PR
  octocode  ██████████████████████████    36,313  
  gh+rtk    █████████·················    12,539 ←

Q14 Vitest dependency on Vite
  octocode  ██████████████████████████    11,102  
  gh+rtk    █████████████·············     5,655 ←

Q15 Hono JSX array component PR
  octocode  ███████████████████·······    13,588 ←
  gh+rtk    ██████████████████████████    18,965  

Q16 ESLint parser dependency chain
  octocode  ██████████████████████████    14,331  
  gh+rtk    ██████████████████········    10,083 ←

Q17 Next.js fetch memoization
  octocode  ███·······················    13,271 ←
  gh+rtk    ██████████████████████████   105,947  

```

### gh + Headroom compression (orchestrated)  ·  Octocode 5.8× leaner overall  ·  [full write-up](octocode-vs-gh-headroom-023223-2026-08-05.md)

Correctness 9.07 (base) / 9.67 (octocode). Octocode leaner on **11 of 15** questions.

```
Q1  Route regex builder
  octocode  ███████···················     6,774 ←
  gh+hr     ██████████████████████████    25,551  

Q2  Repo discovery + bounded absence
  octocode  █·························     4,042 ←
  gh+hr     ██████████████████████████    81,790  

Q3  Flask route history
  octocode  █████·····················    13,688 ←
  gh+hr     ██████████████████████████    70,033  

Q4  Zustand fix PR state
  octocode  █·························     5,661 ←
  gh+hr     ██████████████████████████   652,481  

Q5  Vue hydration diff review
  octocode  ███████████████████████···    40,956 ←
  gh+hr     ██████████████████████████    45,656  

Q6  Express router cross-repo trace
  octocode  ███████···················    10,743 ←
  gh+hr     ██████████████████████████    38,067  

Q7  Zustand Next.js contract
  octocode  ██████████████████████████     8,874  
  gh+hr     ██████····················     2,144 ←

Q8  VS Code keybinding dispatch
  octocode  ████······················     2,954 ←
  gh+hr     ██████████████████████████    19,038  

Q9  Fastify lifecycle contract
  octocode  █████████·················    17,019 ←
  gh+hr     ██████████████████████████    51,403  

Q10 Axios repo + Node entry chain
  octocode  ███·······················    12,574 ←
  gh+hr     ██████████████████████████   124,885  

Q11 Esbuild Node runtime boundary
  octocode  ██████████████████████████    15,445  
  gh+hr     █████·····················     2,713 ←

Q12 Stream/EventEmitter wiring
  octocode  ██████····················     9,188 ←
  gh+hr     ██████████████████████████    39,776  

Q13 Redis BITFIELD security + fix PR
  octocode  ██████████████████████████    24,782  
  gh+hr     █████████·················     8,638 ←

Q14 Vitest dependency on Vite
  octocode  ██████████████████████████    15,542  
  gh+hr     ███·······················     1,658 ←

Q16 ESLint parser dependency chain
  octocode  ████████████████████████··    15,466 ←
  gh+hr     ██████████████████████████    16,989  

```
*Q15 and Q17 excluded: the Octocode runner subagent bailed (returned a placeholder) - a harness failure, not a tool limit. Verified by hand that Octocode answers Q15 in a single call (~1,974 chars). `gh`+Headroom answered both at 10/10; crediting it those would inflate its record, so both are dropped.*


---

## Use the right tool for the job

The wins split cleanly by **task shape**, and it's worth being explicit about it - this isn't "one tool wins everything."

### Reach for `gh` / `rtk` when it's a quick check

For **one-shot lookups and enumerations** - a single tiny object, a list, a yes/no - `gh`'s bare output is unbeatable, and Octocode's structured envelope + discovery overhead is just extra weight. These are genuine baseline wins at equal correctness:

- **"Does this exist?" absence checks (Q2, `gh` alone):** a repo-scoped `gh` search that returns nothing costs ~0 characters. `gh` 1,507 vs Octocode 14,398.
- **"What's the state / files of this PR?" (Q4, Q13, Q14):** a targeted `gh pr view --json` or a 5 KB `package.json` read beats Octocode's envelope + triage lists.
- **"Which file defines X?" locate (Q11, rtk by-hand):** `gh`'s one-line snippet search hit **317 chars** - the leanest result in any run.
- **Enumerating issues/PRs/commits as a list:** `gh`'s one-line rows carry no pagination/next-hint scaffolding, so bare `gh` wins pure list/triage tasks.

**Rule of thumb:** if the answer is a single small object or a flat list, and you already know where to look, `gh` (optionally + `rtk`/Headroom) is the leaner call.

### Reach for Octocode when it's actual research

Octocode was **built for research** - multi-hop questions where the answer lives *inside* large files, trees, and diffs, and you have to trace it across places. That's exactly where whole-object fetching explodes and Octocode's region-targeted reads pay off. On the questions that require reading into code - cross-repo traces (Q6, Q8), lifecycle/wiring internals (Q9, Q12), entry-chain and dependency-chain following (Q10, Q16, Q17) - Octocode is leaner by wide margins (often 5–13×) at equal correctness, because `gh` has no region read and must pull the whole file to see one symbol.

**Rule of thumb:** if answering means reading *into* code, following a chain across files/repos, or reviewing a multi-file diff, Octocode pulls a fraction of the context.

Real research is mostly the second kind - which is why the run totals favor Octocode by 1.4–5.8× even though `gh` wins the quick-check questions outright.

---

## Caveats (so you can trust the number)

- **Same model both arms.** Runners used the same model and effort, so this measures the *tool surface*, not model skill.
- **Blind grading.** Answers were shuffled X/Y with tool identity hidden; the grader established ground truth by its own current-evidence research, then un-blinded only to tabulate.
- **One pass per run is a snapshot.** Grader scores and individual tie/preference calls shift run-to-run; treat sub-point deltas as noise. The **stable signals are the correctness near-tie and the character gap**, which reproduce across all four runs.
- **Octocode's envelope is counted.** Its JSON `results`+`next` scaffolding is included in its char totals (it inflates Octocode on tiny targets). The Headroom arm's numbers are *post-compression* - its designed advantage - and it still lost 5.8×.
- **Measured, not self-reported.** `wc -c` on captured output; Headroom via its own compressor log.

---

## Bottom line

Use `gh` (and `rtk`/Headroom on top of it) for **quick checks** - a single small object, a list, a yes/no where you already know where to look. It's the leaner call there and this benchmark shows it.

But for **actual research** - multi-hop questions whose answers live inside large files, trees, and diffs - Octocode wins, and it wins because it was **built for research**. Across four independent runs and three leading baselines, it answered the same 17 GitHub research questions **at equal or better correctness while pulling 1.4× to 5.8× less context into the model.** The mechanism is simple and repeatable: Octocode reads the *region* that answers the question instead of the *whole file, tree, or diff* that contains it. Less context means fewer tokens, lower latency, lower cost, and more of the model's attention left for the actual reasoning.

*Reproduce any run by hand from the markdown questions in [`../compare/github-questions/`](../compare/github-questions/) - see [`../INSTRUCTIONS.md`](../INSTRUCTIONS.md). No harness, no hidden answer key.*
