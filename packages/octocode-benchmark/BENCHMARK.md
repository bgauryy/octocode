# Benchmark design

This package measures repository research and code understanding through competing CLIs. It does not measure patching or test execution.

> **This is the canonical description of the run flow.** The README, `INSTRUCTIONS.md`, and the skill point here rather than restating it — edit the flow **once, here**.

**Each question is worked by three separate people/agents, each working alone:** Runner A (baseline CLI), Runner B (Octocode CLI), and the Grader. The two runners get the same question, budget, and frozen refs — only the assigned CLI differs — and neither can see the other or the grader. Both answers are finished before the grader (who never saw either runner work) starts, researches independently, grades each on its own, then compares them.

```text
runner A (baseline CLI)  ─┐
                           ├─ two answers, tool names hidden ─→ grader ─→ scored comparison
runner B (Octocode CLI)  ─┘        (three separate people/agents per question)
```

Keeping the roles separate and blind is what makes the numbers trustworthy: don't reuse one person/agent across roles. Questions contain no answer key — the grader establishes ground truth by its own research, so no one is grading against a supplied answer.

Questions live only as markdown under `compare/` — the GitHub matchups share one canonical set in `github-questions/`, and any corpus-local matchup keeps its own `questions/`. The Octocode arm is always `npx octocode tools …`. Every question is worked; contaminated or unresolved ones are reported in a separate diagnostic slice, not dropped. A single pass is a snapshot — repeat it for a stable claim.

---

## Results — what the runs show

> **📊 Full rollup with per-question bar charts: [`results/SUMMARY.md`](results/SUMMARY.md).** Per-run write-ups are in [`results/`](results/). This section is the one-screen takeaway.

**The finding, stated once:** at equal correctness, Octocode pulls far less raw CLI output into the model's context to reach the same graded answer. It reproduces across **4 independent runs** and **3 baselines** (`gh` alone, `gh`+`rtk`, `gh`+Headroom), same 17 GitHub research questions, **same runner model both arms**, blind third-agent grader.

Efficiency is measured in **characters of raw CLI output** (`wc -c` on captured output; the Headroom arm from its own post-compression log). Characters ≈ tokens ÷ ~4, so every ratio holds for tokens too. **Correctness is graded first, chars second** — a confidently-wrong answer can't win on leanness.

| Baseline | Q | Correctness (base / octocode) | Chars — baseline → octocode | Leaner by | Leaner on |
|---|--:|:--:|---|:--:|:--:|
| [`gh` alone](results/octocode-vs-gh-233502-2026-08-05.md) | 17 | 9.94 / 9.94 (tie) | 286,812 → **152,710** | **1.9×** | 11/17 |
| [`gh`+`rtk` (by-hand)](results/octocode-vs-gh-rtk-224513-2026-08-04.md) | 17 | 10.0 / 10.0 (tie) | 242,691 → **168,773** | **1.4×** | 13/17 |
| [`gh`+`rtk` (orchestrated)](results/octocode-vs-gh-rtk-021054-2026-08-05.md) | 17 | 9.18 / 9.38 (octocode ↑) | 1,040,783 → **395,644** | **2.6×** | 10/17 |
| [`gh`+Headroom](results/octocode-vs-gh-headroom-023223-2026-08-05.md) | 15 | 9.07 / 9.67 (octocode ↑) | 1,180,822 → **203,708** | **5.8×** | 11/15 |

**Combined: 2,751,108 chars (baselines) vs 920,835 (Octocode) — 3.0× less context for the same graded answers.**

```text
gh alone        octocode ████████████████··············  152,710   gh     286,812  → 1.9× more
gh + rtk (hand) octocode █████████████████████·········  168,773   gh+rtk 242,691  → 1.4× more
gh + rtk (orch) octocode ███████████···················  395,644   gh+rtk 1,040,783 → 2.6× more
gh + Headroom   octocode █████·························  203,708   gh+hr  1,180,822 → 5.8× more
                         (each bar scaled to its own baseline — the gap is the ratio)
```

### Why Octocode is leaner (the mechanism)

Same model, same question — the difference is what each tool *hands back*. `gh` has no region read, so it fetches whole objects to answer about a part:

| Task | `gh` / `gh`+`rtk` / `gh`+Headroom | Octocode |
|---|---|---|
| Read a symbol in a 2,000-line file | fetch the **whole file** | `matchString` returns the **anchored region** (~1k) |
| Check one `package.json` field | fetch the **whole manifest** | region-read the **exact field block** |
| Inspect a PR | `pr diff` can dump a **9,000-line lockfile** | structured call returns body + **relevant patch**, skips the lockfile |
| Big output | compress it 13–47% *after* fetching | never fetched whole in the first place |

Clearest datapoint — **Q4 (Zustand PR)**, Headroom run: `gh pr diff` pulled the added `package-lock.json` and, *even after Headroom compression*, landed at **652,481 chars**. Octocode answered the identical question in **5,661**. Same answer, ~115× less context. Compressing a whole file you never needed still costs more than reading only the region — which is why the tool whose entire job is compressing `gh` output (Headroom) lost by the **widest** margin.

### Use the right tool for the job

The wins split cleanly by **task shape** — this is not "one tool wins everything":

- **Quick checks → `gh` (± `rtk`/Headroom).** A single small object, a yes/no absence check, a flat list, a one-line locate where you already know where to look. `gh`'s bare output is unbeatable here (it hit **317 chars** on one locate); Octocode's JSON envelope + discovery overhead is just weight.
- **Actual research → Octocode.** Multi-hop questions whose answers live *inside* large files, trees, and diffs — cross-repo traces, lifecycle/wiring internals, entry- and dependency-chain following, multi-file diff review. Here whole-object fetching explodes and Octocode is leaner by **5–13×** at equal correctness. Real research is mostly this kind, which is why run totals favor Octocode even though `gh` wins the quick-check questions outright.

### Read the numbers honestly

- **Same model both arms** — this measures the *tool surface*, not model skill.
- **Blind grader** — answers shuffled X/Y, tool identity hidden, ground truth established by the grader's own research, un-blinded only to tabulate.
- **One pass per run is a snapshot** — sub-point score deltas are noise; the **stable signals are the correctness near-tie and the character gap**, which reproduce across all four runs.
- **Octocode's envelope is counted against it** (its JSON `results`+`next` scaffolding inflates it on tiny targets); the Headroom arm's chars are already *post-compression* and it still lost 5.8×.
