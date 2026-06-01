# Questions

20 code research questions about the `rtk-ai/rtk` repository. Answer each one, in order, using only the tool you were assigned.

There is intentionally no answer key file. The judge independently validates each submitted answer against the live `rtk-ai/rtk` GitHub repository and source code.

**rtk researcher**: clone the repo first — `git clone https://github.com/rtk-ai/rtk /tmp/rtk-bench` — and use it as your local root.

---

### Q1 — Exhaustive callers of `filter_markdown_body`

Find every file in `rtk-ai/rtk` that calls the function `filter_markdown_body`.
List each file path and the line number of the call.
How many total call sites are there?

> *Evaluates exhaustive search behavior when result counts or per-file density are high.*

---

### Q2 — All usages of `RunOptions` builder methods

In `rtk-ai/rtk`, find every call site where a `RunOptions` builder method is used (`.with_tee(...)`, `.stdout_only()`, `.early_exit_on_failure()`, `.no_trailing_newline()`, `.inherit_stdin()`).
How many total call sites exist across all files? List them grouped by method name.

> *Evaluates dense-file search behavior and long-line handling.*

---

### Q3 — Architecture intent in `src/core/runner.rs` comments

Read `src/core/runner.rs` in `rtk-ai/rtk`.
What do the inline comments or doc comments say about:
1. When `skip_filter_on_failure` should be set to `true`?
2. What `RunMode::Passthrough` is intended for?

Answer by citing the relevant comment text rather than inferring only from code logic.

> *Evaluates comment preservation. The answer depends on comments rather than executable code alone.*

---

### Q4 — All TODO and FIXME comments in `src/`

Find every `TODO`, `FIXME`, or `HACK` comment across all files under `src/` in `rtk-ai/rtk`.
For each one, state: the file path, the line number, and the exact comment text.

> *Evaluates comment-search workflows and whether TODO-like annotations remain discoverable.*

---

### Q5 — Filtering taxonomy documented in `src/core/README.md`

Read `src/core/README.md` in `rtk-ai/rtk`.
What does it say about the difference between `Minimal` and `Aggressive` filter levels?
What specific types of content does each level remove?
Quote the relevant section.

> *Evaluates documentation fidelity for Markdown content.*

---

### Q6 — Command category structure under `src/cmds/`

List every subdirectory under `src/cmds/` in `rtk-ai/rtk`.
For each subdirectory, list the `.rs` files it contains (excluding `mod.rs`).
What is the total count of command implementation files?

> *Directory structure test. rtk `ls`/`tree` filters `target`, `vendor`, etc. — but `src/cmds/` should be unaffected. Tests whether structured metadata (counts) is preserved.*

---

### Q7 — Files under `src/discover/` and their purpose

List all files under `src/discover/` in `rtk-ai/rtk`.
For each file, describe its purpose based on its name and any available context.
What is the `src/discover/` module responsible for as a whole?

> *Directory + content test. Combines structure listing with file content reading.*

---

### Q8 — Largest source file by line count

Which `.rs` file under `src/` in `rtk-ai/rtk` has the most lines of code?
State its path and exact line count.
What is its purpose (based on name and content)?

> *Evaluates file metadata and follow-up content inspection for line-count questions.*

---

### Q9 — Five most recently modified files in `src/`

List the 5 most recently modified `.rs` files under `src/` in `rtk-ai/rtk`.
For each, state the file path and the modification timestamp.

> *Evaluates file metadata workflows for recent-change questions. Use commit/retrieval evidence when filesystem mtimes are not meaningful.*

---

### Q10 — PR #2129: the prior fix being re-implemented

Read PR #2129 in `rtk-ai/rtk` (https://github.com/rtk-ai/rtk/pull/2129).
1. What prior fix was this PR re-implementing, and who originally authored that fix?
2. Why was the re-implementation necessary (what changed in the codebase between the original fix and this PR)?
3. What is the `(body contained only badges/images/comments)` fallback note referenced in the PR description?

> *Evaluates PR body, comments, metadata, and diff context together.*

---

### Q11 — The PR that introduced `--ultra-compact` / `-u`

Search the merged PRs in `rtk-ai/rtk` to find the PR that introduced the `--ultra-compact` or `-u` flag.
1. What is the PR number and title?
2. What was the stated motivation for adding this flag?
3. Which commands were updated to support it?

> *Evaluates PR search, pagination, and content retrieval across titles, bodies, and changed commands.*

---

### Q12 — Open PR labels: any breaking changes?

List the labels applied to the 10 most recently opened (or updated) PRs in `rtk-ai/rtk`.
Are there any PRs labeled `breaking-change`, `breaking`, or similar? If so, what do they change?

> *Evaluates PR metadata coverage for labels and recent PR triage.*

---

### Q13 — Full diff filter in `src/cmds/git/diff_cmd.rs`

Read `src/cmds/git/diff_cmd.rs` in `rtk-ai/rtk` completely.
1. What parts of a `git diff` output does rtk keep?
2. What parts does it strip or compress?
3. What is the maximum number of context lines preserved per hunk?

> *Evaluates large-file remote content retrieval and pagination.*

---

### Q14 — `SECURITY.md` threat model

Read `SECURITY.md` in `rtk-ai/rtk` completely.
1. What inputs does rtk consider trusted vs untrusted?
2. What is the stated threat model for command injection?
3. What shell execution patterns are explicitly called out as risk surfaces?

> *Evaluates complete document retrieval for security guidance.*

---

### Q15 — Total `#[test]` functions across all `src/` modules

Count the total number of `#[test]` annotated functions defined in all `.rs` files under `src/` in `rtk-ai/rtk`.
List the top 5 files by test function count, with their counts.
What is the grand total?

> *Evaluates exhaustive test discovery across many files.*

---

### Q16 — Complete `gh` subcommand dispatch table

In `src/cmds/git/gh_cmd.rs`, what is the complete set of `gh` subcommands that rtk intercepts with custom formatting?
For each subcommand, what does rtk's handler do vs passing through raw?
List every `match` arm in the `run()` function's top-level dispatch.

> *Evaluates full-file reading and targeted extraction from a large dispatch table.*

---

### Q17 — The PR that introduced the hooks system

Search merged PRs in `rtk-ai/rtk` for the PR that introduced the hooks system (hook interception / auto-rewrite strategy).
1. What is the PR number?
2. What was the original design rationale described in the PR description or comments?
3. Were there any design alternatives discussed in the PR review?

> *Evaluates PR archaeology using body text, comments, and review discussion.*

---

### Q18 — npm package named `rtk`

Look up the npm package named exactly `rtk`.
1. What does it do?
2. What is its current version and weekly download count?
3. What is its repository URL?
Is there a naming conflict risk with `rtk-ai/rtk`?

> *Evaluates package-registry lookup as a separate capability from CLI output filtering.*

---

### Q19 — Safety annotation comments in `src/`

Find every `// SAFETY:` comment in `src/` of `rtk-ai/rtk`.
For each one, state the file path, line number, and exact comment text explaining the safety invariant.
If none exist, state that and explain why (given `unsafe_code = "deny"` in Cargo.toml).

> *Evaluates comment search for safety annotations. Search tools can find raw comment text even when filtered reads compress comments.*

---

### Q20 — CI checks in `.github/workflows/`

List all GitHub Actions workflow files in `.github/workflows/` of `rtk-ai/rtk`.
For each workflow:
1. What triggers it (push, PR, schedule)?
2. What jobs does it run?
3. Which checks must pass before a PR can be merged?

> *Evaluates remote directory browsing and workflow-file retrieval. Local clones may answer the same facts through a different path.*
