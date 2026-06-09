# Questions

20 code research questions about the `rtk-ai/rtk` repository. Answer each one, in order, using only the tool you were assigned.

There is intentionally no answer key file. The judge independently validates each submitted answer against the live `rtk-ai/rtk` GitHub repository and source code.

**rtk researcher**: clone the repo first — `git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench` — and use it as your local root.

---

Scoring model: see [`benchmark/README.md`](../README.md).

---

## Capability Dimensions

| Dimension | What it tests | Questions |
|---|---|---|
| **Result completeness** — result ceilings and line truncation can affect exhaustive counts | Q1, Q2, Q16 |
| **Comment search** — TODO/SAFETY text is the grep target itself, not stripped context; fair for both tools | Q3, Q4 |
| **Prose/code content** — answer lives in code logic or documentation prose, not inline comments | Q5, Q6, Q7, Q8, Q9 |
| **Comment preservation** — answer lives in inline doc comments that rtk Minimal filter strips | Q10, Q11, Q12 |
| **Directory structure** — listing files and subdirectories | Q13, Q14 |
| **File metadata** — recency, size-class; no comment dependency | Q15, Q17 |
| **PR body + metadata** — PR title, body prose, labels; tests 2000-char passthrough limit | Q18, Q19, Q20 |
| **Exhaustive counting** — exact counts across many files | Q16 |
| **Large file targeted read** — retrieving one logical section of a large file efficiently | Q8, Q9 |

---

## SEARCH — Q1–Q4

### Q1 — Exhaustive callers of `filter_markdown_body` `[SEARCH]`

Find every file in `rtk-ai/rtk` that calls the function `filter_markdown_body`.
List each file path and the line number of the call.
How many total call sites are there?

> *Tests exhaustive search when result density may be high. Both tools can grep; rtk has a configured `grep_max_results` cap that may truncate when call sites are numerous.*

---

### Q2 — All usages of `RunOptions` builder methods `[SEARCH]`

In `rtk-ai/rtk`, find every call site where a `RunOptions` builder method is used:
`.with_tee(...)`, `.stdout_only()`, `.early_exit_on_failure()`, `.no_trailing_newline()`, `.inherit_stdin()`.

How many total call sites exist across all files? List them grouped by method name.

> *Tests dense-file search and long-line handling. The grep target is a method name — not surrounding context — so both tools can find these regardless of comment-stripping. rtk's per-file result cap may truncate in high-density files.*

---

### Q3 — All `TODO`, `FIXME`, and `HACK` comments in `src/` `[SEARCH · COMMENT-AS-TARGET]`

Find every `TODO`, `FIXME`, or `HACK` annotation across all files under `src/` in `rtk-ai/rtk`.
For each one, state: the file path, the line number, and the exact comment text.

> *The annotation text IS the search match — not surrounding context. Both tools retrieve it via grep regardless of comment-stripping policy. Tests whether all annotation lines are returned without truncation.*

---

### Q4 — All `// SAFETY:` comments in `src/` `[SEARCH · COMMENT-AS-TARGET]`

Find every `// SAFETY:` comment in `src/` of `rtk-ai/rtk`.
For each one, state the file path, line number, and the exact comment text explaining the safety invariant.
If none exist, state that and explain why (given that `unsafe_code = "deny"` is declared in `Cargo.toml`).

> *The safety comment text IS the search match. Fair for both tools. Tests exhaustive comment-line retrieval. A correct "none found — because unsafe is denied" answer also receives full credit.*

---

## CONTENT: Prose and Code Logic — Q5–Q9

*These questions are answered from documentation prose or code control flow. Comment stripping does not affect the answer.*

### Q5 — Filter pipeline in `src/core/README.md` `[CONTENT · PROSE]`

Read `src/core/README.md` in `rtk-ai/rtk`.
List the TOML filter pipeline stages in the order they are applied, and describe what each stage does.

> *Small Markdown document. Both tools read it with minimal loss. Tests documentation fidelity for a short structured doc.*

---

### Q6 — Top-level module declarations in `src/lib.rs` `[CONTENT · CODE]`

List every top-level `mod` declaration in `src/lib.rs` of `rtk-ai/rtk`.
For each module, give a one-line description of its responsibility based on its name and a brief content scan.

> *Small entry-point file. Answer is in code structure, not comments. Both tools read it straightforwardly.*

---

### Q7 — `SECURITY.md` threat model `[CONTENT · PROSE]`

Read `SECURITY.md` in `rtk-ai/rtk` completely.
1. What inputs does rtk consider trusted vs untrusted?
2. What is the stated threat model for command injection?
3. What shell execution patterns are explicitly called out as risk surfaces?

> *Medium Markdown document. Prose content is not stripped by either tool's filters. Tests complete document retrieval.*

---

### Q8 — Diff filter logic in `src/cmds/git/diff_cmd.rs` `[CONTENT · CODE · LARGE FILE]`

Read `src/cmds/git/diff_cmd.rs` in `rtk-ai/rtk`.
1. What parts of a `git diff` output does rtk keep?
2. What parts does it strip or compress?
3. What is the maximum number of context lines preserved per hunk?

> *Large Rust file. Answer is in code logic and constants — not inline comments. Tests whether the researcher retrieves the relevant section efficiently (one targeted read) vs. expensively (multiple full-file reads).*

---

### Q9 — `gh` subcommand dispatch table in `src/cmds/git/gh_cmd.rs` `[CONTENT · CODE · LARGE FILE]`

Read `src/cmds/git/gh_cmd.rs` in `rtk-ai/rtk`.
List the complete set of `gh` subcommands that rtk intercepts with custom formatting.
For each subcommand, state what rtk's handler does vs passing through raw.
List every `match` arm in the `run()` function's top-level dispatch.

> *Large Rust file. Answer lives in `match` arm structure — no comment dependency. Rewards targeted reads (`matchString` / `--max-lines`) over full-file fetches.*

---

## CONTENT: Comment Preservation — Q10–Q12

*These questions depend on inline or doc comments. rtk's `Minimal` filter strips comments from `rtk read` output by design. This is a documented tradeoff dimension, not an error.*

### Q10 — Architecture intent in `src/core/runner.rs` `[CONTENT · COMMENT PRESERVATION]`

Read `src/core/runner.rs` in `rtk-ai/rtk`.
What do the inline or doc comments say about:
1. When `skip_filter_on_failure` should be set to `true`?
2. What `RunMode::Passthrough` is intended for?

Answer by citing the relevant comment text, not by inferring from code logic alone.

> *Answer depends on comments that rtk `read` strips at Minimal level. Tests whether the tool preserves architectural intent embedded in source comments.*

---

### Q11 — Doc comments on `FilterLevel` enum variants `[CONTENT · COMMENT PRESERVATION]`

Find the `FilterLevel` enum in `rtk-ai/rtk`.
What does the doc comment (`///`) on each variant say about its behaviour and what it removes?
Quote each variant's doc comment verbatim.

> *Answer lives in `///` doc comments on enum variants. rtk `read` at Minimal level strips these. Tests doc comment access.*

---

### Q12 — `RunOptions` struct doc comments `[CONTENT · COMMENT PRESERVATION]`

Find the `RunOptions` struct definition in `rtk-ai/rtk`.
1. What does the doc comment (`///`) on the struct itself say about its purpose?
2. What do the field-level doc comments say about each field's role?
Quote the relevant doc comments verbatim.

> *`RunOptions` is confirmed to exist (Q2 uses its builder methods). Doc comments on structs and fields are stripped by rtk `read` at Minimal level. Tests whether full-fidelity reads expose struct-level API documentation that compressed reads hide.*

---

## STRUCTURE AND METADATA — Q13–Q17

### Q13 — Command category structure under `src/cmds/` `[STRUCTURE]`

List every subdirectory under `src/cmds/` in `rtk-ai/rtk`.
For each subdirectory, list the `.rs` files it contains (excluding `mod.rs`).
What is the total count of command implementation files?

> *Directory structure question. Both tools list directories and files. Tests metadata completeness and counting accuracy.*

---

### Q14 — Files under `src/discover/` and their purpose `[STRUCTURE · CONTENT]`

List all files under `src/discover/` in `rtk-ai/rtk`.
For each file, describe its purpose based on its name and a brief content scan.
What is the `src/discover/` module responsible for as a whole?

> *Small directory. Combines structure listing with lightweight content inspection. Tests whether brief reads suffice or the researcher over-fetches.*

---

### Q15 — The highest-density `.rs` file under `src/` `[METADATA · CONTENT]`

Which `.rs` file under `src/` in `rtk-ai/rtk` has the most lines of code?
State its path and your best line-count estimate (from any metric available to your toolset).
What is its purpose based on name and content?

> *File-size metadata. octocode: `localFindFiles` returns sizes; line count estimated from size. rtk: `rg "" --count` gives non-empty line counts per file. Both have a valid path to the answer. Tests creative use of available metadata.*

---

### Q16 — Total `#[test]` functions across all `src/` modules `[SEARCH · EXHAUSTIVE COUNT]`

Count the total number of `#[test]` annotated functions defined in all `.rs` files under `src/` in `rtk-ai/rtk`.
List the top 5 files by test function count, with their individual counts.
What is the grand total?

> *Exhaustive count across many files. Tests whether result caps cause undercounting when the total number of matches is high.*

---

### Q17 — Five most recently modified `.rs` files `[METADATA]`

List the 5 most recently modified `.rs` files under `src/` in `rtk-ai/rtk`.
For each, state the file path and the most recent modification timestamp (use commit history or retrieval evidence — filesystem clone times are not meaningful).

> *File recency via commit evidence. Both tools access git history via GitHub API or local clone. Tests metadata workflows for recent-change questions.*

---

## PR AND REMOTE CONTENT — Q18–Q20

*These questions use GitHub API data. rtk routes GitHub operations through `rtk gh` with a `passthrough_max_chars = 2000` cap. Octocode uses structured GitHub API tools with full pagination. Large bodies or YAML files may be truncated on the rtk side.*

### Q18 — PR #2129: the prior fix being re-implemented `[PR · BODY]`

Read PR #2129 in `rtk-ai/rtk` (https://github.com/rtk-ai/rtk/pull/2129).
1. What prior fix was this PR re-implementing, and who originally authored that fix?
2. Why was the re-implementation necessary (what changed between the original fix and this PR)?
3. What is the `(body contained only badges/images/comments)` fallback note referenced in the PR description?

> *PR body content question. Body length tests the 2000-char passthrough cap. Tests PR prose retrieval accuracy.*

---

### Q19 — The PR that introduced `--ultra-compact` / `-u` `[PR · SEARCH]`

Search the merged PRs in `rtk-ai/rtk` to find the PR that introduced the `--ultra-compact` or `-u` flag.
1. What is the PR number and title?
2. What was the stated motivation for adding this flag?
3. Which commands were updated to support it?

> *PR search + body retrieval. Tests multi-step PR archaeology: search to find the right PR, then retrieve its body for motivation and changed files.*

---

### Q20 — Labels on the 10 most recently updated PRs `[PR · METADATA]`

List the labels applied to the 10 most recently opened or updated PRs in `rtk-ai/rtk`.
Are there any PRs labeled `breaking-change`, `breaking`, or similar?
If so, what do those PRs change?

> *PR label metadata. Tests whether structured metadata (labels) survives the retrieval path. rtk `gh pr list` passes through label fields within the passthrough window; octocode returns labels in structured output.*
