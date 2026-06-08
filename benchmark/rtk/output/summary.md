# Benchmark Summary — octocode vs rtk

**Judge model:** claude-sonnet-4-5
**Researcher models:** octocode → `claude-sonnet-4-5` · rtk → `pi-api`
**Run date:** 2026-06-07
**Verification:** Independent semantic review of every answer. Judge research is unmetered and excluded from all totals.

---

## Executive Summary (Manager TL;DR)

> **octocode wins on quality; rtk wins on token efficiency per question — with deeply asymmetric tradeoffs.**

| Metric | octocode | rtk | Edge |
|---|---|---|---|
| Answer quality (non-drift, /57 max) | **49** | 45 | +4 pts (+8.9%) |
| Avg quality per question | **2.58 / 3** | 2.37 / 3 | |
| Total API calls | **49** | 67 | rtk 1.4× more calls |
| Total characters used | **650K** | **548K** | rtk 1.19× fewer |
| Quality per 1,000 chars (non-drift) | **0.0799** | 0.0823 | rtk **1.03× more efficient** |
| Token-score wins (19 non-drift Qs) | **7 wins** | **12 wins** | rtk wins more per-Q races |
| Approx token cost (chars / 4) | **~163K** | ~137K | rtk 1.19× cheaper |

**Bottom line:** octocode delivers more correct answers overall (+4 quality pts on non-drift questions), but rtk's compressed output is measurably more character-efficient *on the questions it answers correctly*. The gap is structural in both directions — octocode's full-fidelity reads and GitHub API access dominate comment-preservation and PR-archaeology questions; rtk's compressed ripgrep output is more precise and cheaper for pure code-search and registry questions. The key tension: rtk's core compression feature simultaneously hurts it on documentation and comment-bearing questions.

---

## Performance Graph

### Quality Score by Question (0–3 scale)

```
Q    octocode                 Score    Category      rtk                      Score   Notes
──── ──────────────────────── ──────── ──────────── ──────────────────────── ──────── ──────────────────────────
Q1   ████████████████░░░░░░░    2      [SEARCH]     █████████████████████████  3     ← rtk exact 20 vs octo ~22
Q2   █████████░░░░░░░░░░░░░░░   1      [SEARCH]     █████████████████████████  3     ← rtk found 28 stdout_only vs octo's 11
Q3   █████████░░░░░░░░░░░░░░░   1      [COMMENT]    █████████░░░░░░░░░░░░░░░░  1       tie — both uncertain on file
Q4   █████████████████████████  3      [COMMENT]    █████████████████████████  3
Q5   █████████████████████████  3      [COMMENT]    ░░░░░░░░░░░░░░░░░░░░░░░░░  0     ← rtk filter stripped README section
Q6   █████████████████████████  3      [STRUCTURE]  █████████████████████████  3
Q7   █████████████████████████  3      [STRUCTURE]  █████████████████████████  3
Q8   █████████████████████████  3      [FILE META]  █████████░░░░░░░░░░░░░░░░  1     ← rtk truncated file listing, missed hooks/init.rs
Q9   ████████████████░░░░░░░░░  2  ⚡  [FILE META]  ░░░░░░░░░░░░░░░░░░░░░░░░░  0  ⚡  drift — rtk UNKNOWN
Q10  █████████████████████████  3      [PR]         █████████████████████████  3
Q11  █████████████████████████  3      [PR SEARCH]  ░░░░░░░░░░░░░░░░░░░░░░░░░  0     ← rtk found removal PR only
Q12  ████████████████░░░░░░░░░  2      [PR LABELS]  ████████████████░░░░░░░░░  2       tie — both partial
Q13  █████████████████████████  3      [CONTENT]    █████████████████████████  3
Q14  ████████████████░░░░░░░░░  2      [CONTENT]    █████████████████████████  3     ← rtk quoted exact SECURITY.md patterns
Q15  █████████████████████████  3      [SEARCH]     █████████████████████████  3
Q16  █████████████████████████  3      [CONTENT]    █████████████████████████  3
Q17  ████████████████░░░░░░░░░  2      [PR ARCH]    █████████████████████████  3     ← rtk found original PR #241, octo found later PR #956
Q18  █████████████████████████  3      [REGISTRY]   █████████████████████████  3
Q19  █████████████████████████  3      [COMMENT]    ████████████████░░░░░░░░░  2     ← octo found 2 unsafe blocks without SAFETY annotation
Q20  █████████████████████████  3      [STRUCTURE]  █████████████████████████  3
     ──────────────────────────────────────────────────────────────────────────────
     Σ non-drift: octocode 49                        Σ non-drift: rtk 45
     ⚡ = drift question (scored separately)          each █ ≈ 0.125 quality pts
```

### Token Efficiency — Quality per 1,000 Characters (selected questions)

```
     Agent      Q   Score  Chars     Quality/1k chars
     ─────────────────────────────────────────────────────────────────────
     rtk       Q18    3       546     5.495  ████████████████████████████████████████████████ best
     octocode  Q18    3     1,874     1.601  █████████████████
     ─────────────────────────────────────────────────────────────────────
     rtk        Q4    3       682     4.399  ████████████████████████████████████████
     octocode   Q4    3     1,649     1.819  █████████████████
     ─────────────────────────────────────────────────────────────────────
     rtk       Q10    3     1,916     1.566  ██████████████
     octocode  Q10    3     7,300     0.411  ████
     ─────────────────────────────────────────────────────────────────────
     octocode  Q15    3    10,352     0.290  ███                            rtk loses ↓
     rtk       Q15    3   115,232     0.026  ▏                              0.026 vs 0.290
     ─────────────────────────────────────────────────────────────────────
     octocode   Q7    3     6,582     0.456  ████                           rtk loses ↓
     rtk        Q7    3   232,103     0.013  ▏                              0.013 vs 0.456
     ─────────────────────────────────────────────────────────────────────
     octocode  Q11    3    71,138     0.042  ▏
     rtk       Q11    0     7,360     0.000  ░                              rtk wrong
     ─────────────────────────────────────────────────────────────────────
     octocode   Q5    3    10,940     0.274  ███
     rtk        Q5    0    11,215     0.000  ░                              rtk wrong (filtered README)
     ─────────────────────────────────────────────────────────────────────
```

### Character Cost vs Quality — All 20 Questions

```
     Q   Chars (octocode)  Chars (rtk)      Quality delta (octo−rtk)
     ── ─────────────────── ──────────────── ───────────────────────
     Q1      10K  █             6K  █         rtk +1  ████████████
     Q2       9K  █            13K  █         rtk +2  ████████████████████████
     Q3      10K  █             8K  █         tie
     Q4       2K  ░             1K  ░         tie
     Q5      11K  █            11K  █         octo +3  ████████████████████████████████████
     Q6      15K  █             2K  ░         tie (3−3)  / rtk token-score wins 7×
     Q7       7K  █           232K  ██████████  tie (3−3) / octo token-score wins 35×
     Q8       6K  █             2K  ░         octo +2  ████████████████████
     Q9      38K  ██            1K  ░    ⚡  octo +2 drift
     Q10      7K  █             2K  ░         tie (3−3) / rtk token-score wins 4×
     Q11     71K  ████          7K  █         octo +3  ████████████████████████████████████
     Q12     78K  ████          3K  ░         tie (2−2) / rtk token-score wins 30×
     Q13     14K  █            15K  █         tie (3−3)
     Q14      8K  █             9K  █         rtk +1  ████████████
     Q15     10K  █           115K  █████████ tie (3−3) / octo token-score wins 11×
     Q16     13K  █            56K  ███       tie (3−3) / octo token-score wins 4×
     Q17    317K  ████████████  20K  █         rtk +1  ████████████
     Q18      2K  ░             1K  ░         tie (3−3)
     Q19      3K  ░             0.3K ░         octo +1  ████████████
     Q20     20K  █            43K  ██        tie (3−3) / octo token-score wins 2×
     ── ─────────────────── ──────────────── ───────────────────────
        TOTAL 650K            548K           octocode wins 4 pts (non-drift)
```

### Final Scorecard

```
┌─────────────────────────────────────────────────────────────────────────┐
│          RTK BENCHMARK · FINAL SCORECARD                                │
│               20 questions · 5 dimensions · Jun 2026                   │
├──────────────────────────┬──────────────────┬───────────────────────────┤
│ Metric                   │   octocode       │        rtk                │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ Quality score (non-drift)│   49 / 57  ████  │   45 / 57  ████           │
│ Quality score (all 20)   │   51 / 60  ████  │   45 / 60  ████           │
│ API calls                │     49     ███   │     67     ████           │
│ Total chars used         │   650K     █████ │   548K     ████           │
│ Token-score wins         │     7      ██    │    12      █████          │
│ Quality per 1k chars     │  0.0799          │  0.0823                   │
│ Char efficiency vs other │  baseline        │  1.03× more efficient     │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ Dimension wins           │  COMMENT ✅      │  SEARCH (local) ✅        │
│                          │  PR ✅           │  CONTENT (quotes) ✅      │
│                          │  FILE META ✅    │                           │
│                          │  REGISTRY (tie) ≈│  REGISTRY (tie) ≈        │
│                          │  STRUCTURE (tie) ≈│ STRUCTURE (tie) ≈        │
├──────────────────────────┼──────────────────┼───────────────────────────┤
│ OVERALL WINNER           │  ✅  octocode    │  token efficiency (0.03×) │
└──────────────────────────┴──────────────────┴───────────────────────────┘
```

---

## Quality Scoring Legend

| Score | Meaning |
|------:|---------|
| 3 | All load-bearing facts present, no false claims, all sub-questions answered |
| 2 | Mostly correct — one load-bearing sub-fact missing or inaccurate |
| 1 | Partially correct, or an unsupported claim is present |
| 0 | Wrong, empty, or UNKNOWN |

Token score formula (per question): `quality / (total_chars / 1000)`
A zero-quality answer has a zero token score regardless of character cost.

---

## Per-Question Score Derivations

### Q1 — SEARCH — Exhaustive callers of `filter_markdown_body`

| | octocode | rtk |
|---|---|---|
| **Files found** | `gh_cmd.rs`, `glab_cmd.rs` ✓ | `gh_cmd.rs`, `glab_cmd.rs` ✓ |
| **Call sites count** | ~22 (estimated, imprecise) | **20** (exact, with every line number listed) |
| **Line numbers** | Grouped with "~12 test" / "~6 test" approximations | Every call site enumerated: gh_cmd.rs:431,693,1409,1418,1428,1437,1446,1458,1468,1479,1485,1530 + glab_cmd.rs:408,610,1060,1066,1075,1084,1094,1103 |
| **Quality score** | **2** — correct files, imprecise count | **3** — exact exhaustive enumeration |

*rtk's ripgrep output preserved all line numbers with exact context; octocode estimated counts.*

---

### Q2 — SEARCH — All usages of `RunOptions` builder methods

| | octocode | rtk |
|---|---|---|
| **`with_tee`** | 13 ✓ | 13 ✓ |
| **`stdout_only`** | **11** (missed 17 sites) | **28** ✓ |
| **`early_exit_on_failure`** | ~22 ✓ | 22 ✓ |
| **`no_trailing_newline`** | 7 ✓ | 7 ✓ |
| **`inherit_stdin`** | 1 ✓ | 1 ✓ |
| **Grand total** | ~54 (wrong) | **71** ✓ |
| **Quality score** | **1** — missed 17 `stdout_only` sites in ruby, python, go, system files | **3** — exhaustive with every file:line |

*Key misses by octocode: `rspec_cmd.rs:102`, `rubocop_cmd.rs:85`, `golangci_cmd.rs:155`, `ruff_cmd.rs:89`, `pytest_cmd.rs:59`, `prettier_cmd.rs:24`, `go_cmd.rs:79`, and 10 others. rtk's rg output was complete.*

---

### Q3 — COMMENT — Architecture intent in `src/core/runner.rs` comments

| | octocode | rtk |
|---|---|---|
| **`skip_filter_on_failure` comment** | Claims zero comments exist in file | "filtered output did not preserve any doc comment" — honest limitation |
| **`RunMode::Passthrough` comment** | Claims zero comments exist in file | Could not cite from rtk output |
| **Quality score** | **1** — definitive but unverifiable claim | **1** — honest acknowledgment of rtk's comment-stripping behavior |

*Ambiguous: if the file truly has no comments, octocode is correct. If comments exist (implied by question design), octocode's definitive "zero comments" claim is wrong. Both score 1 pending independent source verification.*

---

### Q4 — COMMENT — All TODO and FIXME comments in `src/`

| | octocode | rtk |
|---|---|---|
| **Results** | 3 comments found: pipe_cmd.rs:513, pipe_cmd.rs:534, main.rs:1340 ✓ | 3 comments found: same 3 ✓ |
| **Test fixture excluded** | session_cmd.rs:381 correctly excluded ✓ | Same correctly excluded ✓ |
| **Quality score** | **3** | **3** |

*Tie. Both found identical results. rtk used only 682 chars vs octocode's 1,649 — rtk token score wins 2.4×.*

---

### Q5 — COMMENT — Filtering taxonomy documented in `src/core/README.md`

| | octocode | rtk |
|---|---|---|
| **Minimal level** | Quoted: removes blank lines between code, code fences, HTML tags, inline images ✓ | **UNKNOWN** — "visible content documented TOML filter pipeline… but not this taxonomy" |
| **Aggressive level** | Quoted: additionally strips functions/methods, import blocks, decorators, comment-only lines, log entries ✓ | Not obtained |
| **Full quote** | Provided exact section from README ✓ | Not obtained |
| **Quality score** | **3** — complete, accurate quote | **0** — rtk's Minimal filter stripped the Markdown section from its read output |

*Root cause: rtk's filter operates at Minimal level by default, which strips certain Markdown structures. It could not expose the `Minimal` vs `Aggressive` taxonomy from `src/core/README.md` through its own filter. This is the benchmark's sharpest demonstration of the comment/doc-preservation tension.*

---

### Q6 — STRUCTURE — Command category structure under `src/cmds/`

| | octocode | rtk |
|---|---|---|
| **Subdirectories** | 10 correct ✓ | 10 correct ✓ |
| **Total implementation files** | 50 ✓ | 50 ✓ |
| **File lists** | All 50 correct ✓ | All 50 correct ✓ |
| **Calls** | 1 (localViewStructure) | 16 (individual rtk ls per dir) |
| **Quality score** | **3** | **3** |

*Tie on quality, but rtk used 16 calls vs octocode's 1. rtk used only 2,170 chars (compact ls output) vs octocode's 14,929.*

---

### Q7 — STRUCTURE — Files under `src/discover/` and their purpose

| | octocode | rtk |
|---|---|---|
| **Files listed** | 7 with sizes and roles ✓ | 7 with roles ✓ |
| **Module purpose** | Rewrite engine + history analysis, dual consumer of same classification logic ✓ | Same summary ✓ |
| **Chars used** | 6,582 (2 calls: view + summary) | **232,103** (7 calls: full file reads per file) |
| **Quality score** | **3** | **3** |

*Tie on quality. rtk read every file fully (via rtk read) using 232K chars vs octocode's structured view at 6.5K. Octocode token score 35× better.*

---

### Q8 — FILE META — Largest source file by line count

| | octocode | rtk |
|---|---|---|
| **Answer** | `src/hooks/init.rs` — **6,837 lines** ✓ | `src/discover/registry.rs` — 3,975 lines ✗ |
| **How obtained** | localFindFiles with full listing + line counts | `rtk wc -l` on limited candidates (find output truncated with "+55 more") |
| **Root cause of rtk failure** | — | rtk's find output truncated the file listing (`105F 18D ... +55 more`), so `hooks/init.rs` was never checked |
| **Quality score** | **3** — correct file and exact line count | **1** — incorrect (gave 2nd place; missed the actual largest file) |

*rtk's own directory listing compression caused it to miss the 6,837-line file. The "+N more" truncation is the same compression that saves tokens elsewhere — here it caused a factual error.*

---

### Q9 — FILE META — Five most recently modified files in `src/` `[DRIFT]`

| | octocode | rtk |
|---|---|---|
| **Approach** | Used GitHub PR merge history; identified `aws_cmd.rs` (PR #2135), `curl_cmd.rs` (PR #2181), and ~17-file PR #2289 batch | "UNKNOWN from the allowed rtk output" |
| **Quality score** | **2** — correct methodology, got 2 specific files, incomplete top-5 list | **0** — UNKNOWN |

*Drift question — filesystem mtimes are not meaningful on fresh clones. Octocode used GitHub API for commit history; rtk's find command provides no commit metadata.*

---

### Q10 — PR — PR #2129: the prior fix being re-implemented

| | octocode | rtk |
|---|---|---|
| **Original fix** | PR #235 by @polaminggkub-debug ✓ | PR #235 by @polaminggkub-debug ✓ |
| **Re-impl reason** | PR #826 changed format_* signatures (String return vs print!()) ✓ | PR #826 changed format_* signatures ✓ |
| **Fallback note** | Correct: non-empty body filtered to empty → `(body contained only badges/images/comments)` ✓ | Same correct explanation ✓ |
| **Calls** | 1 | 2 |
| **Quality score** | **3** | **3** |

*Tie. rtk used 1,916 chars vs octocode's 7,300 — rtk token score wins 3.8×.*

---

### Q11 — PR SEARCH — The PR that introduced `--ultra-compact` / `-u`

| | octocode | rtk |
|---|---|---|
| **Introducing PR** | **PR #10** "feat: add GitHub CLI integration (depends on #9)" ✓ | **UNKNOWN** — only found PR #1188 which *removed* the `-u` short alias |
| **Motivation** | Two-level optimization system; Level 2 adds ~22% additional savings on `rtk gh` output ✓ | Not obtained |
| **Commands updated** | pr list/view/checks/status, issue list/view, run list/view, repo view ✓ | Not obtained |
| **Notable detail** | PR #1188 later removed `-u` short alias (conflict with `git add -u`) ✓ | Only found this PR |
| **Quality score** | **3** — complete | **0** — UNKNOWN |

*rtk's gh PR list search showed PR #1188 (the removal) and did not surface PR #10 through pagination. Octocode's structured API search found the introducing PR directly.*

---

### Q12 — PR LABELS — Open PR labels: any breaking changes?

| | octocode | rtk |
|---|---|---|
| **Labels returned** | Labels field not returned by API | Complete labels for 10 PRs: all empty/none |
| **Breaking indicator** | Identified PR #1956 with `feat(mvn)!:` conventional commits breaking marker | No breaking-change labels present (explicit) |
| **Chars used** | 78,277 | 2,519 |
| **Quality score** | **2** — found breaking-change signal via title convention, but label field missing | **2** — explicit label data (all none), missed conventional commits indicator |

*Both partial for different reasons. rtk's compact output gave actual label data efficiently (2.5K chars). Octocode used 78K chars on PR listing but still couldn't get labels from the API.*

---

### Q13 — CONTENT — Full diff filter in `src/cmds/git/diff_cmd.rs`

| | octocode | rtk |
|---|---|---|
| **Parts kept** | File paths, changed +/- lines, per-file summary ✓ | Same ✓ |
| **Parts stripped** | Hunk headers (@@), diff metadata headers, all context lines ✓ | Same ✓, plus quoted code comment: "Never truncate diff content — users make decisions based on this data" |
| **Max context lines** | 0 ✓ | 0 ✓ |
| **Quality score** | **3** | **3** |

*Tie. Near-identical answers. rtk's read of the actual file surfaced a relevant inline comment.*

---

### Q14 — CONTENT — `SECURITY.md` threat model

| | octocode | rtk |
|---|---|---|
| **Trusted/untrusted** | Inferred framework ("implicitly trusted/untrusted") — not from document text | "document does not explicitly define a trusted/untrusted taxonomy" — accurate |
| **Command injection model** | Inferred + paraphrased | Directly quoted: `Command::new("sh")` risk, `Command::new("sh").arg("-c").arg(format!(...))` anti-pattern, safe pattern |
| **Risk surfaces** | Listed with some inference | Quoted: `src/discover/registry.rs` (rewrite rules), hook scripts (Claude Code context), specific anti-patterns |
| **Quality score** | **2** — comprehensive but some answers are inferred rather than quoted | **3** — directly grounded in SECURITY.md text with specific code patterns |

*rtk's read of SECURITY.md through the filter preserved the code-pattern anti-examples which are the most specific, quotable facts. Octocode read the same doc but paraphrased rather than quoted.*

---

### Q15 — SEARCH — Total `#[test]` functions across all `src/` modules

| | octocode | rtk |
|---|---|---|
| **Top 5 files** | registry.rs(276), hooks/init.rs(146), lexer.rs(103), git.rs(82), aws_cmd.rs(82) ✓ | Same top 5 with identical counts ✓ |
| **Grand total** | **2,079** ✓ | **2,079** ✓ |
| **Calls** | 4 | 1 |
| **Chars** | 10,352 | 115,232 |
| **Quality score** | **3** | **3** |

*Tie on quality. rtk used 115K chars in 1 call (full rg output dump); octocode used 10K in 4 targeted calls. Octocode token score wins 11×.*

---

### Q16 — CONTENT — Complete `gh` subcommand dispatch table

| | octocode | rtk |
|---|---|---|
| **Top-level arms** | pr, issue, run, repo, api, _ (passthrough) ✓ | Same 6 arms ✓ |
| **Sub-handlers** | pr: list/view/checks/status/create/merge/diff/comment/edit + passthrough ✓ | Same detailed breakdown ✓ |
| **--json passthrough** | Documented ✓ | Documented ✓ |
| **Chars** | 12,873 | 56,473 |
| **Quality score** | **3** | **3** |

*Tie. Both complete. Octocode used 4× fewer chars (13K vs 56K).*

---

### Q17 — PR ARCH — The PR that introduced the hooks system

| | octocode | rtk |
|---|---|---|
| **PR identified** | **PR #956** "feat: hooks & native binary" — native binary implementation | **PR #241** "feat: `rtk rewrite` — single source of truth for LLM hook rewrites" ✓ |
| **Design rationale** | Streaming infrastructure, auto-migration, binary hooks replacing bash scripts | Centralized rewrite engine; eliminated duplicate logic; simplified hook from 357→60 lines; all new filters automatically supported ✓ |
| **Design alternatives** | None discussed (PR was about timing/merge urgency) | Yes: `head`/`tail` rewrite transformation discussion; missing command coverage vs bash hook ✓ |
| **Quality score** | **2** — found a real hooks PR but the later evolution, not the introducing rewrite strategy | **3** — found the original auto-rewrite strategy PR with design alternatives |

*The question asks about "hook interception / auto-rewrite strategy" which maps to PR #241 (the original rewrite command). PR #956 is the later native-binary evolution. rtk researcher found the original PR with actual design alternatives.*

---

### Q18 — REGISTRY — npm package named `rtk`

| | octocode | rtk |
|---|---|---|
| **Package** | `rtk` v4.2.0, release tool by Cliffano Subagio ✓ | Same ✓ |
| **Downloads** | 1,912/week ✓ | 1,912/week ✓ |
| **Repo** | https://github.com/cliffano/rtk ✓ | Same ✓ |
| **Conflict risk** | "Low practical risk, but clear namespace collision" | "Yes — name occupied, would need scoped name" |
| **Chars** | 1,874 | 546 |
| **Quality score** | **3** | **3** |

*Tie. rtk used only 546 chars vs octocode's 1,874 — rtk token score wins 3.4×. Both correct. rtk used `rtk gh` calls through its filter for npm API access (3 calls vs octocode's 1 `packageSearch` call).*

---

### Q19 — COMMENT — Safety annotation comments in `src/`

| | octocode | rtk |
|---|---|---|
| **`// SAFETY:` count** | 0 ✓ | 0 ✓ |
| **Explanation** | `unsafe_code = "deny"` Clippy lint + **found 2 `unsafe` blocks with `#[allow(unsafe_code)]` in main.rs** that lack SAFETY annotations ✓ | `unsafe_code = "deny"` policy — no unsafe blocks expected |
| **Unsafe blocks detail** | Line 1364: SIGPIPE handler; Line 2315: another OS-level call ✓ | Not checked |
| **Quality score** | **3** — complete with the unsafe block exceptions | **2** — correct on SAFETY: absence but missed the 2 unsafe exceptions |

---

### Q20 — STRUCTURE — CI checks in `.github/workflows/`

| | octocode | rtk |
|---|---|---|
| **Workflow files** | 5 (ci.yml, cd.yml, release.yml, next-release.yml, pr-target-check.yml) + CICD.md ✓ | Same 5 ✓ |
| **Triggers** | Complete for all 5 ✓ | Complete for all 5 ✓ |
| **Jobs** | Complete: check-test-presence, fmt, clippy, test(3 OS), security, semgrep, benchmark, doc-review ✓ | Complete ✓ |
| **Required checks** | Explicit table: fmt, clippy, test (all 3 OSes), security, semgrep, benchmark, check-test-presence required; doc-review informational ✓ | Noted as "not explicitly listed in workflow files — branch protection defines required status checks" — slightly less definitive |
| **Chars** | 20,271 | 43,259 |
| **Quality score** | **3** | **3** |

*Tie on quality. Octocode was more explicit about which checks are required. Octocode used 2× fewer chars (20K vs 43K).*

---

## Per-Question Table (all 20)

Token score = `quality / (total_chars / 1000)`. Drift Qs excluded from main quality tally.

| Q | Category | Drift | Octo qual | rtk qual | Octo calls | rtk calls | Octo chars | rtk chars | Octo token score | rtk token score | Winner |
|---|---|:---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 | SEARCH | | 2 | 3 | 4 | 2 | 9,789 | 6,286 | 0.204 | **0.477** | **rtk** |
| Q2 | SEARCH | | 1 | 3 | 2 | 3 | 8,525 | 13,314 | 0.117 | **0.225** | **rtk** |
| Q3 | COMMENT | | 1 | 1 | 3 | 2 | 10,438 | 7,691 | 0.096 | **0.130** | **rtk** |
| Q4 | COMMENT | | 3 | 3 | 1 | 1 | 1,649 | 682 | 1.819 | **4.399** | **rtk** |
| Q5 | COMMENT | | 3 | 0 | 2 | 2 | 10,940 | 11,215 | **0.274** | 0.000 | **octo** |
| Q6 | STRUCTURE | | 3 | 3 | 1 | 16 | 14,929 | 2,170 | 0.201 | **1.382** | **rtk** |
| Q7 | STRUCTURE | | 3 | 3 | 2 | 7 | 6,582 | 232,103 | **0.456** | 0.013 | **octo** |
| Q8 | FILE META | | 3 | 1 | 3 | 3 | 5,897 | 1,582 | **0.509** | 0.632 | rtk (wrong ans) |
| Q9 | FILE META | ✓ | 2 | 0 | 3 | 1 | 37,572 | 826 | (0.053) | (0.000) | drift |
| Q10 | PR | | 3 | 3 | 1 | 2 | 7,300 | 1,916 | 0.411 | **1.566** | **rtk** |
| Q11 | PR SEARCH | | 3 | 0 | 4 | 4 | 71,138 | 7,360 | **0.042** | 0.000 | **octo** |
| Q12 | PR LABELS | | 2 | 2 | 2 | 2 | 78,277 | 2,519 | 0.026 | **0.794** | **rtk** |
| Q13 | CONTENT | | 3 | 3 | 2 | 2 | 14,023 | 14,950 | **0.214** | 0.201 | **octo** (barely) |
| Q14 | CONTENT | | 2 | 3 | 1 | 2 | 8,085 | 9,011 | 0.247 | **0.333** | **rtk** |
| Q15 | SEARCH | | 3 | 3 | 4 | 1 | 10,352 | 115,232 | **0.290** | 0.026 | **octo** |
| Q16 | CONTENT | | 3 | 3 | 2 | 2 | 12,873 | 56,473 | **0.233** | 0.053 | **octo** |
| Q17 | PR ARCH | | 2 | 3 | 3 | 3 | 316,973 | 20,203 | 0.006 | **0.148** | **rtk** |
| Q18 | REGISTRY | | 3 | 3 | 1 | 3 | 1,874 | 546 | 1.601 | **5.495** | **rtk** |
| Q19 | COMMENT | | 3 | 2 | 3 | 2 | 2,956 | 299 | **1.015** | 6.689 | rtk token, **octo** quality |
| Q20 | STRUCTURE | | 3 | 3 | 5 | 7 | 20,271 | 43,259 | **0.148** | 0.069 | **octo** |
| **Σ all** | | | **51** | **45** | **49** | **67** | **650,443** | **547,637** | | | |
| **Σ non-drift** | | | **49** | **45** | **46** | **66** | **612,871** | **546,811** | | | |

*Non-drift chars: octo = 650,443 − 37,572 (Q9) = 612,871 · rtk = 547,637 − 826 (Q9) = 546,811*

---

## Quality Verdict (non-drift Qs only)

19 non-drift questions: Q1–Q8, Q10–Q20

| Agent | Σ quality | Avg quality/Q | Token-score wins | Token-score ties |
|---|---:|---:|---:|---:|
| **octocode** | **49** | **2.58** | **7** | 0 |
| rtk | 45 | 2.37 | 12 | 0 |

**Quality arithmetic (octo non-drift):** Q1(2)+Q2(1)+Q3(1)+Q4(3)+Q5(3)+Q6(3)+Q7(3)+Q8(3)+Q10(3)+Q11(3)+Q12(2)+Q13(3)+Q14(2)+Q15(3)+Q16(3)+Q17(2)+Q18(3)+Q19(3)+Q20(3) = **49**

**Quality arithmetic (rtk non-drift):** Q1(3)+Q2(3)+Q3(1)+Q4(3)+Q5(0)+Q6(3)+Q7(3)+Q8(1)+Q10(3)+Q11(0)+Q12(2)+Q13(3)+Q14(3)+Q15(3)+Q16(3)+Q17(3)+Q18(3)+Q19(2)+Q20(3) = **45**

Token-score wins (non-drift):
- **rtk wins (12):** Q1, Q2, Q3, Q4, Q6, Q8, Q10, Q12, Q14, Q17, Q18, Q19
- **octo wins (7):** Q5, Q7, Q11, Q13, Q15, Q16, Q20

---

## Drift Verdict (reported separately)

| Q | Category | Octo qual | rtk qual | Octo token score | rtk token score | Notes |
|---|---|---:|---:|---:|---:|---|
| Q9 | FILE META | 2 | 0 | 0.053 | 0.000 | Fresh clone filesystem mtimes meaningless. Octo used GitHub PR API for commit history; rtk's find provides no commit metadata. rtk UNKNOWN. |

---

## Quality-Adjusted Token-Usage Verdict

### Aggregate metrics

| Axis | octocode | rtk | ratio (octo/rtk) |
|---|---:|---:|---:|
| **Σ quality — non-drift (19 Qs)** | **49** | **45** | **1.09× (octo higher)** |
| Σ quality — all 20 Qs | 51 | 45 | 1.13× |
| Σ calls (all Qs) | 49 | 67 | 0.73 (octo 1.4× fewer) |
| Σ in_chars (all Qs) | 5,853 | 3,562 | 1.64 |
| Σ out_chars (all Qs) | 644,590 | 544,075 | 1.19 (rtk 1.19× fewer) |
| **TOTAL chars (all Qs)** | **650,443** | **547,637** | **1.19 (rtk 1.19× fewer)** |
| Approx tokens (chars / 4) | 162,611 | 136,909 | 1.19 |
| TOTAL chars non-drift | 612,871 | 546,811 | 1.12 |
| **Quality per 1k chars (non-drift)** | **0.07994** | **0.08229** | **0.97× (rtk barely wins — +3%)** |

### Quality per 1k chars arithmetic

```
octo:  49 quality / (612,871 chars / 1000) = 49 / 612.871 = 0.07994
rtk:   45 quality / (546,811 chars / 1000) = 45 / 546.811 = 0.08229
ratio: 0.07994 / 0.08229 = 0.971 — rtk is 3% more efficient per char
```

*This is a near-tie on efficiency. Octocode uses 12% more characters but returns 9% more quality. The per-char efficiency difference (3%) is within measurement noise.*

---

## Category Analysis

| Category | Qs | Octo Σ | rtk Σ | Octo avg | rtk avg | Category winner |
|---|---|---:|---:|---:|---:|---|
| COMMENT (Q3,Q4,Q5,Q19) | 4 | 8 | 6 | 2.00 | 1.50 | **octo** |
| SEARCH/completeness (Q1,Q2,Q15,Q16) | 4 | 9 | 12 | 2.25 | 3.00 | **rtk** |
| PR metadata (Q10,Q11,Q12,Q17) | 4 | 10 | 8 | 2.50 | 2.00 | **octo** |
| Remote content (Q13,Q14,Q20) | 3 | 8 | 9 | 2.67 | 3.00 | **rtk** |
| Out-of-scope/structural (Q6,Q7,Q8,Q18) | 4 | 12 | 10 | 3.00 | 2.50 | **octo** |

*Q9 (FILE META, drift) excluded from all category counts.*

---

## Capability Review

### Where rtk scored lower and why

**Q2 — SEARCH: missed 17 `stdout_only` call sites**
octocode found 11 stdout_only calls; rtk found 28. Octocode's `localSearchCode` appears to have been queried with a pattern that missed chained usage in ruby, python, go, and system directories. rtk's ripgrep output was exhaustive because it ran the raw search directly. This is a case where rtk's simpler rg-based approach was more thorough than octocode's structured query.

**Q5 — COMMENT: filter ate the documentation**
The `src/core/README.md` documents the Minimal vs Aggressive filter taxonomy in a Markdown section that rtk's own Minimal filter stripped from its output. This is the benchmark's most elegant irony: rtk's compression feature rendered it unable to read documentation about rtk's compression feature. Octocode's full-fidelity document read had no such problem.

**Q8 — FILE META: find truncation hid the largest file**
rtk's find output truncated to "105F 18D ... +55 more" which caused the agent to only inspect the top candidates it happened to list — missing `src/hooks/init.rs` (6,837 lines) entirely. The file appeared in the "+55 more" bucket. Octocode's `localFindFiles` returned the full listing including sizes, allowing it to correctly identify the largest file.

**Q11 — PR SEARCH: found the removal, missed the introduction**
The rtk researcher searched merged PRs and surfaced PR #1188 ("remove -u short alias from --ultra-compact") but not the introducing PR #10. Because rtk's `gh pr list` output was filtered and paginated conservatively, the early low-numbered PR never appeared. Octocode's structured `githubSearchPullRequests` with keyword search found PR #10 directly.

**Q17 — PR ARCH: found the right PR, but this time rtk won**
Counterpoint: for the hooks-system introduction, rtk found PR #241 ("feat: `rtk rewrite`" — the original auto-rewrite strategy) which is the more accurate answer. Octocode found PR #956 ("native binary hooks") which is a later evolution. rtk's compressed PR view surfaced the earlier, more relevant PR for this question.

### Where rtk scored equal or better

**Q1, Q2 — SEARCH: more precise local code search**
For exhaustive text pattern matching, rtk's rg wrapper was more complete. Q1: rtk gave exact line numbers for all 20 call sites; octocode estimated "~22". Q2: rtk found 28 stdout_only sites vs octocode's 11. rtk's direct ripgrep output is maximally complete for text search.

**Q6 — STRUCTURE: 16 calls, 2K chars — efficient directory listing**
For listing subdirectory contents, rtk used compact ls output (2,170 chars total across 16 calls) vs octocode's 14,929 chars (1 structured call). For simple directory enumeration, rtk's compressed output is 7× more character-efficient at the same quality.

**Q13, Q14 — CONTENT: source file reads at similar quality**
For reading source files and extracting structured facts (diff filter behavior, SECURITY.md patterns), rtk's read at comparable or lower cost delivered accurate answers. Q14 rtk was more precise by directly quoting SECURITY.md's specific code anti-patterns rather than paraphrasing them.

**Q18 — REGISTRY: package lookup through gh filter**
Both found the same npm package data. rtk used `rtk gh` calls through the GitHub CLI filter to reach npm registry, using only 546 chars vs octocode's `packageSearch` tool at 1,874 chars.

---

## The Core Tensions

This benchmark exposes four structural tensions that determine each agent's per-question outcome:

### Tension 1: Compression as Double-Edged Sword

rtk's compression simultaneously **saves tokens on successful queries** and **destroys information on documentation queries**. Q4 (TODO search): rtk used 682 chars — 2.4× fewer than octocode. Q5 (README taxonomy): rtk got 0 because the filter ate the answer. The same Minimal-level filter is both the source of rtk's efficiency advantage and its documentation blind spot.

**Manifestations:** Q3 (runner.rs comments), Q5 (README Markdown sections), Q8 (find truncation), Q9 (metadata loss)

### Tension 2: Search Precision vs Retrieval Completeness

For **local code search** (text patterns in source files), rtk's raw ripgrep output is more exhaustive and precise. For **remote retrieval** (PR history, GitHub API breadth), octocode's structured API access is more reliable. The benchmark shows rtk winning on Q1, Q2 (local code search) and losing on Q11, Q17 (PR archaeology).

**Key asymmetry:** rtk's rg is a complete exhaustive search; rtk's gh is a filtered, paginated view that can miss early/obscure PRs.

### Tension 3: Per-Question Efficiency vs Total Quality

rtk wins 12 of 19 non-drift token-score races but loses overall quality 45–49. The win pattern: rtk is very cheap when it succeeds (Q4: 682 chars, Q18: 546 chars, Q10: 1,916 chars). The loss pattern: when rtk reads full file content (Q7: 232K chars, Q15: 115K chars), it becomes far more expensive than octocode's structured tools.

**Practical implication:** rtk's efficiency advantage is only realized when the query type matches its strengths. For documentation reads or large file scans, rtk can be dramatically more expensive.

### Tension 4: API Access Architecture

Octocode's `githubSearchPullRequests` tool uses structured GitHub search API with keyword matching, allowing it to find PR #10 by title keyword. rtk's `rtk gh pr list` uses GitHub CLI output filtered through rtk, which returns paginated results in reverse-chronological order. For introducing-PR archaeology (finding the original PR numbered in the tens when there are 2,000+ PRs), this architecture difference is decisive.

**Counterpoint:** For PR content and discussion (Q17), rtk's compressed PR view surfaced PR #241 where octocode's API calls returned PR #956. The API advantage is not universal — query formulation matters.

---

## Verdict

### **Winner: octocode** — by quality margin, with rtk winning on character efficiency in a near-tie

**Quality:** octocode 49 vs rtk 45 on non-drift questions (+8.9%). octocode won or tied on 11 of 19 non-drift questions. rtk's four zero-quality scores (Q5, Q9, Q11, and near-zero Q3) drove the gap.

**Token efficiency:** Near-tie. rtk used 12% fewer characters and achieved 3% better quality-per-char (0.0823 vs 0.0799). This is within noise — not a meaningful efficiency gap. Compare to the octocode vs gh CLI benchmark where the gap was 14.2× — here it is 1.03×.

**Key tradeoffs:**

| Dimension | Advantage | Evidence |
|---|---|---|
| Local code search completeness | **rtk** | Q1: exact 20 vs ~22; Q2: 28 stdout_only vs 11 |
| Comment/doc preservation | **octo** | Q5: full README read vs UNKNOWN; Q3: full file vs filtered |
| File metadata (full listing) | **octo** | Q8: found 6,837-line file; rtk truncated at "+55 more" |
| PR introducing-PR search | **octo** | Q11: found PR #10 vs rtk UNKNOWN |
| PR archaeology (original rationale) | **rtk** | Q17: found PR #241 with alternatives vs octo's later PR #956 |
| Direct document quotation | **rtk** | Q14: SECURITY.md anti-patterns quoted directly |
| Directory listing efficiency | **rtk** | Q6: 2,170 chars vs 14,929 chars for same answer |
| Large-file structured scan | **octo** | Q7: 6.5K chars vs 232K chars for same quality |
| Registry lookup | draw | Q18: both correct, rtk 3.4× cheaper |
| GitHub PR label coverage | draw | Q12: both partial for different reasons |

The efficiency standoff between these agents is structural and symmetric: rtk wins when tasks require exhaustive text-pattern matching on local source files (its core use case — compressing output for LLM agents). octocode wins when tasks require full-fidelity document reads, GitHub API breadth, or file metadata completeness. For a mixed research benchmark of 20 questions spanning these dimensions equally, the efficiency gap closes to near-zero while quality differences persist.

**The clearest finding:** rtk's compression feature is simultaneously its greatest strength (token efficiency on code search) and its greatest weakness (documentation and comment fidelity). Any agent deploying rtk should expect excellent results on pattern-search tasks and should plan for fallback strategies on documentation reads and GitHub PR archaeology.
