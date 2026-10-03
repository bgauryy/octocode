# rg-gh: reflection after 30 questions

# REFLECT.md

Based on 30 code-research sessions: G01–G10 on GitHub PRs and issues via `gh`, and L01–L20 on local checkouts via `rg` and `sed`.

## What helped

- **Batching the pin check with the first search (L01–L05, L08–L10, L14, L15, L19).** Running `git rev-parse HEAD` or `git log -1` in the same Bash call as a broad `rg` confirmed the pinned SHA and located the mechanism at once. Most local questions were answered in two or three calls.
- **Combined `gh` calls for PR and issue questions (G01, G03, G04, G08, G09, G10).** `gh pr view --json ...` together with `gh pr diff`, sometimes plus `gh issue view --json`, returned intent, merge SHA, file list and code change in one round trip.
- **Saving the diff to a file (G01, G02, G04).** `gh pr diff > /tmp/d.txt`, indexed with `grep -n '^diff'` and sliced with `sed` or `awk`, avoided truncation and let me read specific files.
- **New tests in a PR diff (G02, G03).** They state the expected behavior directly.
- **Pinned-commit file fetch (G05, G06, G07).** `gh api "repos/OWNER/REPO/contents/PATH?ref=SHA"` (base64-decoded or with the raw Accept header) gave reliable line numbers without a clone.
- **`rg -n` first, then `sed -n` on the hit ranges (L01, L02, L06, L08, L10, L17).** This gave function bodies and call sites cheaply.
- **Excluding tests and docs with globs (L05, L06, L19, L20)** cut noise.
- **Guessing file locations from repo layout (L17, L18)** worked and saved exploration calls.

## What did not help

- **Unquoted `?` in `gh api` URLs (G05, G07).** zsh globbed it and the call errored, costing a round trip.
- **`gh issue view --comments | head -100` (G08, G09).** It printed nothing, so both needed a rerun with `--json`.
- **Truncating with `head -c` or `head` (G01, G10, L04, L09).** Diffs were cut mid-file, or grep output lost callers, forcing refetches.
- **`sed` output has no line numbers.** I derived citations by counting from range starts (L10, L13, L14, L04, L07). Several are approximate (`~`), and some I did not flag as approximate (L13).
- **Diff hunks give no absolute line numbers (G01, G04)**, so I could not cite `path:line` at the pinned commit.
- **Searching too narrowly first.** Restricting to `tools/` (L04), `scrape.go` (L08) or guessed files (L01) missed definitions and cost extra calls. In L14, guessing a per-function file failed.
- **Noisy matches.** `app-page-runtime.ts` had about 10 near-identical hits (L06). `isCounter` hits were irrelevant (L10). `diagnostics/loc/*.generated.json` flooded the output (L20). Checker and transformer hits were noise (L19).
- **Speculative or redundant calls.** These included `rg lifo task/mod.rs` returning nothing (L12), a `find` in local directories that found nothing (G06), overlapping `sed` reads (L09, L11, L15), and a `gh pr diff` call with unsupported path args (G04).
- **Unread code behind claims (G05–G10, L03, L05, L06, L09–L13, L15–L20).** Examples are `_api.py` (G01), the pre-PR code (G02), `merge_content`'s body (L05), `sendEtagResponse` (L06), the `manager.go` callers (L09), and `histogramRate` (L10). Several of these claims went out as inference. Most gaps were disclosed, but not all: L12 disclosed only one of several, and L17's `DataMember` claim rested on a grep hit.
- **Transcription error (G09).** The merge SHA in the answer had a stray space.
- **`cd` in every call** (L03, L09, L19) shifted the working directory, and the shell cwd kept resetting (G06).

## Patterns

- **Two-to-three-call sessions were the norm and mostly sufficient.** The recurring miss was verification, not discovery. I found the code with `rg`, then cited or characterized parts I had only seen as grep hits or comments.
- **Line-number precision was the most common weakness.** It came from `sed` without numbering, diff-relative lines, and `~` ranges.
- **Follow-up context was skipped.** I rarely read PR review comments or linked issues (G01, G03, G10), tests (G04, L20), or the pre-change code (G02).
- **Discovery of the right file or directory took extra calls when the first search was scoped too narrowly.** Package-wide searches from the start avoided this.
- **Output handling caused repeat calls.** These were `head` truncation, `--comments | head` and unquoted URLs.

## Suggested changes

1. **Require numbered output for every cited line.** Instruct: use `rg -n -C`, or `nl -ba file | sed -n 'a,bp'`, and never estimate line numbers. Mark any unverified line explicitly. This addresses the most frequent flaw.
2. **Add a "read before you cite" rule.** If a claim depends on a function body, read it. Grep hits and names are not evidence. List every inferred claim under uncertainty, not only the largest gap.
3. **Give a `gh` recipe in the instructions.**
   - Always quote URLs.
   - Use `--json` (never `--comments | head`).
   - Save diffs to a `mktemp` file rather than truncating.
   - Use `gh api "repos/O/R/contents/P?ref=SHA"` for absolute line numbers at a commit.
4. **Standardize the first call.** Combine the pin check (`git rev-parse HEAD`) with a package-wide `rg -n --type <lang>`, with tests, docs and generated or localization files excluded by glob. Do not start with narrow file guesses.
5. **Add a closing-check step.** Before answering, run one more targeted read for each claim flagged as inferred, such as callers, definitions and adjacent tests. Where it is cheap, also pull PR review comments or linked issues.
6. **Use absolute paths and drop `cd`.** Copy SHAs from tool output instead of retyping them.
7. **Optional tooling.** A helper that prints a file range with line numbers and a wrapper for "PR summary plus saved diff plus file index" would remove most of the repeated friction.

---

# Per-question reflections

## G01

**1. Helped:**
- The first parallel pair was `gh pr view 16403 --json ...` and `gh pr diff`. The view call gave the file list, the merged state, the head SHA and the PR body. The diff call gave `routing.py` and `applications.py`.
- Saving the diff to `/tmp/p.diff` let me slice it with `awk` and `sed`. That is how I read `_runtime.py` and `pyproject.toml` in full.

**2. Did not help:**
- The first `head -c 30000` truncated the diff in the middle of `_api.py`, so I had to re-fetch it.
- The diff-relative line numbers (from `sed -n` slices of `p.diff`) can't be cited as `path:line` at the pinned commit.
- I never read `_api.py` or most of `_asgi.py`. My `grep` over `_asgi.py` was a keyword filter, so I saw only matching lines.
- I never checked out the repo at the pinned commit. `gh` can't give file lines at a commit.

**3. Next time:**
- Fetch the diff to a file on the first call.
- Read `_api.py` fully, since `_unconfigured` and `_operation` decide the cost of the no-SDK path.
- Use `gh api` with the contents endpoint at the head SHA to get real file line numbers.
- Check the PR's review comments, which I never looked at.

**4. Confidence:** medium. I read the behavior summary, the `_runtime.py` code and the dependency changes directly. The no-SDK cost, the redaction list and how `exclude` works for mounts are unverified, and I said so in the answer.

## G02

1. **Helped:** Saving the PR diff with `gh pr diff 13824 > /tmp/d.txt`, then running `grep -n '^diff'` to index it. I could then `sed` straight to the files that mattered: `validators/counter.rs`, `input_python.rs`, `_generate_schema.py`, `_known_annotated_metadata.py`, `_validators.py`, and `tests/types/test_counter.py`. The new tests were the most useful part, because they state the expected behavior directly.

2. **Did not help:**
   - The first `gh pr view --json ... | head -c` call dumped truncated file-list JSON and got persisted to a file, so I got little from it. I had to make a second call for the merge SHA.
   - I never looked at the pre-PR code. I did not check out or view `_mapping_schema` at the parent commit, so the "before" behavior is inferred from the removed lines, and I said so in the answer.
   - I did not run the tests or look for follow-up commits.

3. **Next time:** I would fetch the base-commit source with `gh api` or `git show`, or check whether a local pydantic checkout exists. That would let me confirm the old error type, strict-mode behavior and constraint handling. I would also search the diff for removed test expectations in `tests/types/test_counter.py`, which show the old behavior.

4. **Confidence:** High for the after-PR behavior, since I read the diff and the tests. Medium for the before/after comparison, because the "before" side is inferred. It is not verified.

## G03

1. **Helped:** I made one Bash call that ran `gh pr view 5881 -R nodejs/undici --json title,body,state,mergeCommit,files` followed by `gh pr diff 5881`. It returned the PR rationale, the merge SHA and the full diff together. That was enough to answer, and the added test file confirmed the intended behavior.

2. **Did not help:** Nothing failed and I made no repeated calls. The diff was long, mostly the 228-line test file, so it cost tokens. The diff shows only changed hunks. It did not show `kRemoveClient`'s callers or the `clientTtl` eviction code. That means my claim about the TTL path is inferred from the tests and comments, not seen directly. I also didn't open the PR's review comments or a linked issue, so I have no maintainer discussion of the root cause.

3. **Next time:** I would run `gh pr view` with `--json comments,reviews` and `gh api` for the linked issue. I would also `rg kRemoveClient` against a checkout, or fetch the file at the merge SHA, to confirm the TTL eviction path.

4. **Confidence:** High for the core mechanism, because the PR body and the diff agree with each other. Medium for the `clientTtl` detail, because I never saw that code. I also didn't run the tests.

## G04

1. **Helped:** The first call, `gh pr view 3866 -R pallets/click --json title,body,state,mergeCommit,files,url`, gave the PR's intent, its merge SHA and its file list in one shot. The second call fetched the full diff and filtered it with `awk` down to `src/` and `CHANGES.md`. That call showed the real check logic (`_check_name_is_usable`, `_check_name_is_normalized`) and where each check is called. The diff was the authoritative source, so I didn't need a checkout.

2. **Did not help:** The second command was clumsy. It ran `gh pr diff` twice: first with `-- src/click/core.py CHANGES.md`, which `gh pr diff` doesn't support and which I silenced with `2>/dev/null`, then again with the `awk` filter. That was redundant, and the first attempt's output was either empty or unclear. Diff hunks give no absolute line numbers, so I couldn't cite `path:line` at the pinned commit. I also never opened the tests, so I didn't confirm the warning behavior by running it. The changelog wording and the docs wording differ slightly, and I didn't reconcile them.

3. **Next time:** I'd fetch the diff once and pipe it through `awk`. To get real line numbers, I'd run `gh api` on the file contents at the merge SHA `06b2a678`, or `rg -n` in a local checkout. I'd also skim `tests/test_deprecations.py` to confirm the exact warning cases.

4. **Confidence:** High on what is deprecated and which declarations warn, because it comes straight from the code diff and the PR body. Medium on completeness, because I didn't read the tests or check for follow-up changes.

## G05

1. **Helped:** Fetching `sessions.py` with `gh api "repos/psf/requests/contents/src/requests/sessions.py?ref=611c6162cb"` piped through `base64 -d` into `/tmp/sessions.py`. That pinned the exact commit and gave me one file. The `grep -n "def resolve_redirects|def rebuild_..."` and the `sed -n 95,400p` read covered the whole redirect path in about two calls. A final `grep` for `resolve_redirects|allow_redirects` connected `Session.send` to the loop.

2. **Did not help:**
   - My first call errored. I didn't quote the URL, so zsh tried to glob the `?`. That cost one round trip.
   - The same call also listed the unrelated local `repos/python` directory. That was noise, and I never used the local checkouts.
   - The two closing greps could have been one call.

3. **Next time:** Quote the `gh api` URL from the start. Combine the `send` wiring grep into the first pass. Check `models.py` for `is_redirect`, which I skipped and flagged as a gap.

4. **Confidence:** High for the mechanics and line numbers. They come from the pinned-commit file I read directly, and I cited only lines I saw. Medium-high overall, because I did not run the code and did not verify which status codes count as redirects.

## G06

1. **Helped:** The `gh api` call that downloaded `httpx/_client.py` at the pinned SHA was the fastest route. Saving it to `/tmp/c.py` let me search it locally. The `rg` for `proxy|proxies|mounts|_transport_for_url` pointed me straight to lines 239, 685–716, 760 and 1005. Fetching `_utils.py` the same way gave me `get_environment_proxies` and `URLPattern`. Because I pinned the SHA, the line numbers are reliable.

2. **Did not help:** My first `find` and `ls` of the local repos directory found nothing for httpx, so that call was wasted. The shell cwd also kept resetting, so I had to `cd /tmp` every time. My `rg -A75 | rg` filter on `URLPattern` showed only the docstring lines, so I had to make a second fetch with `sed 162,235p`. That second fetch is why the `priority` line range in my answer is approximate (`~200-210`) when I could have given it exactly. I also did not open `_init_proxy_transport` (lines 740–758), so I never saw how the proxy transport is constructed.

3. **Next time:** I would run `gh api` for the pinned SHA first, without searching local directories. I would print exact ranges with `sed -n` or `rg -n` and skip the piped greps. I would cite exact line numbers for every claim.

4. **Confidence:** High on the overall mechanism, because I read the code for every step. Medium-high on the exact line numbers. Only the `priority` range is approximate.

## G07

1. **Helped:** The second Bash call fixed my first failure. It used `gh api "repos/Kludex/starlette/contents/starlette/$f.py?ref=63c5760d8a"` with the raw Accept header. That pinned the commit and fetched four files in one loop, so I didn't need a clone. The third call was a single command that printed `_exception_handler.py`, `exceptions.py`, and the `__call__` in `errors.py` with line numbers. That gave me all the citations I needed.

2. **Did not help:** My first call failed because zsh tried to glob the unquoted `?`. It also wrote files into `/tmp`, which already held unrelated scripts, and the `wc -l *.py` listing was noise. I never checked the local checkout directories. I also never fetched `routing.py` or `RequestBodyLimitMiddleware`. Because of that, the answer doesn't cover per-route exception wrapping or how the router raises.

3. **Next time:** I'd quote the URLs from the start. I'd use a fresh `mktemp` directory. I'd add `rg -n "wrap_app_handling_exceptions"` across the fetched files, or fetch `routing.py`, to verify the call sites.

4. **Confidence:** High for the stack order and the handler lookup, because I read the code directly at the pinned commit. Medium for completeness, because I skipped the router and the body-limit middleware, and I said so in the answer.

## G08

1. **Helped:** The second Bash call did most of the work. It combined `gh issue view 18837 --json title,body,state,comments`, `gh pr view 18838 --json body,files,mergeCommit` and `gh pr diff 18838`. That returned the issue's repro, the PR's stated intent, and the actual `proxy.js` change in one round trip. The first call's `gh pr list --search "18837" --json ...` found PR #18838 directly.

2. **Did not help:** In the first call, `gh issue view --comments | head -100` printed nothing. Only the JSON from `gh pr list` appeared, so I had to re-run the issue view. I never found out why it was empty. I didn't use the local checkouts, and they weren't Svelte anyway. I never looked at the post-merge `proxy.js`, so I have no line numbers for it and I never saw the `has` trap's code.

3. **Next time:** I'd use `--json` output from the start instead of piping `--comments` through `head`. I'd also run `gh api` or `gh search code` against the merge commit `a72dc8ea` to read the `has` trap and get `path:line` citations.

4. **Confidence:** High on the root cause and the fix, because the issue body, the PR description and the diff all agree. Medium-high on my claim that the old code created no dependency where no source existed. I inferred that from the diff and didn't verify it in the full file. I ran no tests, and I said so in the answer.

## G09

1. **Helped:** The second call, `gh issue view 13786 --json title,body,state,url,comments`, returned the whole issue body. That body contained the root cause, a reproduction and the affected code path. The first call's `gh pr list --search "13786"` found the candidate PRs (#13787, #13794, #13825). `gh pr view 13825 --json ...` gave the merge commit and file list. `gh pr diff 13825` showed the actual fix.

2. **Did not help:** The first call's `gh issue view --comments | head -100` printed nothing, so I had to repeat it with `--json`. I cut the diff with `sed -n 1,200p` and never read the rest, meaning `config.py`, `deprecated/decorator.py` and the tests. I did not check the merged code on `main`. I also did not open the closed PRs #13787 and #13794. My guess that #13787 was the issue author's patch is unverified, though I said so.

3. **Next time:** I would use `--json` from the start. I would read the full diff, or at least the test hunks. I would open PR #13787 briefly to check who wrote it.

4. **Confidence:** Medium-high on the root cause and the fix, because the issue text and the diff agree. One error to correct: in my answer, the merge SHA `5da36b5de4f44a572ca5c12104fd2f8669fd feca` has a stray space and may be mistyped. Use the value from the `gh pr view` output, not my transcription.

## G10

**1. Helped:** The first Bash call, `gh pr view 8156 --json ...` piped into `gh pr diff`, gave me the PR body, the file list and the full diff in one round trip. The body already named the two open Miri issues (#5047 and #5054). The second call filtered the truncated remainder of the diff with `awk` and `grep` for miri-related added and removed lines. That produced the list of remaining ignores and their reasons.

**2. Did not help:**
- The first call cut the diff off at 20KB with `head -c`, so I needed the second call. I should have filtered for miri lines from the start.
- The grep output had no surrounding context. I couldn't tell which test each ignore belonged to, or how many tests were ignored.
- I never opened the linked Miri issues, so I don't know if they have been fixed.
- I never checked out the code, so nothing was verified against the actual file at the merge commit.

**3. Next time:** I'd run one `gh pr diff | grep -B3 -E 'miri'` to keep test names attached to each ignore. I would also run `gh issue view` on rust-lang/miri#5047 and `gh pr view` on #5054 to check their current status. To confirm the remaining gates I would read the files at the merge commit, for example with `gh api`.

**4. Confidence:** Medium-high. The changes and the stated reasons come straight from the diff and PR body. The per-test lists are not exhaustive, and the issue status is unchecked.

## L01

1. **Helped:** My first Bash call combined `git rev-parse HEAD` with one `rg` for `APPEND_SLASH|should_redirect_with_slash|get_full_path...`, restricted to three files. It confirmed the pinned commit and located every relevant line at once. The second call, a `sed` of `common.py` lines 30-125 plus `request.py` lines 215-245, gave me the full logic. That was two calls in total, and I already knew where to look.

2. **Did not help:** Nothing errored and I made no repeated calls. My first `rg` was restricted to guessed files, so it could have missed usages elsewhere. I did not open `is_valid_path` in `django/urls`. I also did not find where `should_append_slash` is set, for example the decorator that disables the redirect. I said both in my answer. The `cd` in each call was redundant, since the working directory changed anyway.

3. **Next time:** I'd add a repo-wide `rg` for `should_append_slash` and `def is_valid_path`, in parallel with the file reads. That would close both gaps.

4. **Confidence:** High for the main flow: the 404 check in `process_response`, the conditions in `should_redirect_with_slash`, and the path building. I read that code directly at the pinned commit. Medium-high for the peripheral claims about the opt-out attribute and `is_valid_path` behavior, which I did not verify.

## L02

1. **Helped:** The first Bash call combined `git rev-parse HEAD` with `rg -n "unsaved related object|_prepare_related_fields_for_save" django`. It confirmed the pinned commit and found the message string, the method definition and its callers in one step. The second call, `sed -n 1270,1340p base.py` plus `sed -n 858,868p`, gave the whole method body and the `save()` call site. That was enough to answer.

2. **Did not help:** Nothing errored and I made no repeated calls. The `| head` on the `rg` output could have cut off callers, though only five lines came back. I did not open `query.py:1042`, so I could not say which operation that caller belongs to. I also did not check for other places that might raise a similar error, such as `bulk_update` or related-manager paths. I also did not check the tests or git history for context.

3. **Next time:** I would drop `head` and read the `query.py:1042` context with a small `sed`. I would also run a broader `rg` for "prohibited to prevent data loss" to be sure no other check exists.

4. **Confidence:** High for the main claim, that `_prepare_related_fields_for_save` at `base.py:1276`, called from `save()` at `base.py:864`, raises `ValueError`. I read the code directly at the confirmed SHA. Medium-high for completeness, because I did not scan for other checks.

## L03

1. **Helped:** The first Bash call, `git rev-parse HEAD` plus `rg "run_on_commit|on_commit|run_and_clear_commit_hooks" django/db`, confirmed the pinned commit. It also mapped every storage, discard and run site in one pass. The `rg` on `run_commit_hooks_on_set_autocommit_on|run_on_commit` then showed the commit → flag → `set_autocommit` chain. Reading `Atomic.__exit__` in `transaction.py` showed the outermost-block commit path.

2. **Did not help:**
   - My `sed` ranges were sloppy, so the output mixed unrelated code, such as the `set_autocommit` body. Because of that I had to cite lines from the `rg` output instead of from the code I had read.
   - I never viewed the `set_autocommit(True)` call in `Atomic.__exit__`'s `finally` block. I never viewed the full rollback method around `base.py:341` either. Both went into the answer as inferred or flagged gaps.
   - The `cd` calls kept shifting the working directory. That was noise, not an error.

3. **Next time:** I would use `rg -n -C4` on the specific hits instead of guessing `sed` ranges. I would also read the `finally` block of `Atomic.__exit__` to the end, so the full chain is verified. And I would use absolute paths without `cd`.

4. **Confidence:** medium-high. The storage, savepoint-discard and run mechanics were read directly at the pinned commit. The gaps I flagged are the inferred `set_autocommit(True)` step and the exact line of the rollback reset.

## L04

1. **Helped:** The first Bash call did the most. It ran `git rev-parse HEAD` to confirm the pinned commit, then a broad `rg` for `parse_docstring`, `create_schema_from_function` and `_infer_arg_descriptions` across `convert.py`, `base.py` and `structured.py`. That gave me the whole call chain in one output. The `sed -n` range reads that followed gave me the function bodies and line numbers directly. The `rg -A` on `_parse_google_docstring` and `_create_subset_model` located the last two hops, which lived in `utils/`, outside the directory I had been searching.

2. **Did not help:**
   - I searched only `tools/` at first, so I needed extra calls to find `utils/function_calling.py` and `utils/pydantic.py`.
   - The second-to-last `rg` output was truncated by `head`. I read the v2 line numbers from `_create_subset_model`'s output rather than opening the function.
   - The `_create_subset_model_v1` and v2 bodies (`utils/pydantic.py:205-276`) were never opened. I only saw grep fragments of them.
   - Several cited line numbers, such as `base.py:~362-368` and `convert.py:~320-326`, are approximate because I derived them from `sed` ranges instead of viewing exact lines.

3. **Next time:** I'd run `rg` over the whole `langchain_core` package from the start. I'd also open `_create_subset_model_v2` directly and use `rg -n` to pin exact line numbers.

4. **Confidence:** High on the overall flow, which I read in source. Medium-high on the exact line numbers, since a few are approximate. I did not run any code.

## L05

1. **Helped:** The first Bash call did most of the work. It ran `git log -1` to confirm the pinned SHA, then `rg -n "merge_content"` over the source (tests excluded), then a second `rg` over the tests. That one call gave every definition, call site and export. The second Bash call used `sed` to read `base.py:366-380`, `base.py:435-460`, `ai.py:655-670` and the test at `test_messages.py:1085-1112`. That separated the two-argument call sites from the spread ones (`base.py:453`, `ai.py:665`).

2. **Did not help:** I never read the body of `merge_content` (past `base.py:380`). That left the question of whether a pairwise fold gives identical results, and I could only flag it as unverified. I also didn't search outside `libs/core`, so I can't say whether other packages in the repo call the function. The `sed` output cut the parametrize decorator, so I inferred the `others` shape in the test from the visible cases. None of the calls errored, and I made no repeated calls.

3. **Next time:** I'd add `sed -n 380,440p base.py` to the second call to read the merge logic. I'd also run `rg merge_content` across the whole repo, not just `libs/core`.

4. **Confidence:** High for the call-site list, since it came straight from `rg` and the code I read. Medium for the claim that the tests would only need a small change, because I didn't see the top of the test's parametrize block. The fold-equivalence point is unverified.

## L06

1. **Helped:** The first `rg -n "generateEtags"` call, with globs excluding tests, docs and markdown, gave every relevant call site in one pass. The second call combined `sed` ranges on `send-payload.ts`, `router-server.ts` and `base-server.ts`. It showed the actual ETag logic and the static-file `etag` option together, so I needed only two calls.

2. **Did not help:** The `rg` output was noisy in `app-page-runtime.ts`, which had about 10 near-identical hits. I never opened `sendEtagResponse`, `generateETag` or `serve-static.ts`. My answer describes what `sendEtagResponse` does from its name and call site, and I said so in the answer. The last `rg -i etag` in the second call returned only comments, so it added little. The working directory changed between calls, which was harmless.

3. **Next time:** I would add one more call, `rg -n "sendEtagResponse|generateETag|export.*serveStatic" -A15`, to read those definitions. That would replace the inference with evidence, and it would also confirm the `send` mapping.

4. **Confidence:** High for the routing of the flag and the `send-payload.ts` gating logic, because I read that code directly. Medium for the exact behavior of `sendEtagResponse` and `serveStatic`, which I inferred from names and call sites.

## L07

1. **Helped:** The first Bash call did most of the work. It combined `cat client/components/redirect.ts | head -120` with an `rg` for `isRedirectError|getURLFromRedirectError|getRedirectStatusCodeFromError` across `server/`. That gave me the throw site and every catch site in one pass. The second call read the specific line ranges in `app-render.tsx`, `app-route/module.ts`, `action-handler.ts` and `make-get-server-inserted-html.tsx`. It also read `redirect-error.ts`.

2. **Did not help:** Not much was wasted, since I made only two calls. I never opened `createRedirectRenderResult`, so how `x-action-redirect` is set is unverified, and I said so in the answer. I also did not confirm the commit or check `git log`. The checkout was assumed to be at the pinned SHA. The `sed` ranges were guessed from `rg` hits, so I did not confirm the exact line numbers around 4386 and 9833.

3. **Next time:** I would use `rg -n` with `-C` on `createRedirectRenderResult` and `x-action-redirect`. I would also run `git rev-parse HEAD` to confirm the pin, and use `nl -ba` when quoting line numbers.

4. **Confidence:** Medium-high on the main flow: the digest, the 307/308 status, the `location` header, the route-handler 303 for actions, and the MPA 303. Medium on the exact line numbers. I cited some as approximate and did not verify them with numbered output.

## L08

1. **Helped:** The first Bash call did the most work. `git log -1 --format=%H` confirmed the checkout was at the pinned commit. In the same call, `rg -n -i "sampleLimit|errSampleLimit|SampleLimit" scrape/scrape.go` returned the whole flow: config copy, `appenderWithLimits`, `checkAddError` and the metric increment. The second call used `sed -n` on specific ranges of `scrape/scrape.go` and `scrape/target.go` to read the `limitAppender` code. That gave exact line numbers and the stale-marker bypass.

2. **Did not help:** The first `rg` covered only `scrape.go`, so the `errSampleLimit` definition and the V2 appender in `target.go` and `scrape_append_v2.go` turned up only later. That cost a third call. Nothing errored. I did not check the tests or the `up` metric handling, and I said so in the answer.

3. **Next time:** I would run `rg` across the whole `scrape/` directory, excluding `_test`, in the first call. I would also look at the caller of the scrape error to see how a failed scrape sets `up`.

4. **Confidence:** High for the enforcement mechanism, because I read every cited line directly. Medium for completeness. The `up` behaviour and the wider scrape-failure path are not traced.

## L09

1. **Helped:** The first Bash call was the most useful. It ran `git log -1` to confirm the pinned SHA, and one `rg` for `StaleNaN|endOfRunStaleness|forEachStale|iterDone` gave me the whole map of `scrape.go`. Reading the `sed` ranges around `updateStaleMarkers`, `endOfRunStaleness`, `append` and `iterDone` then confirmed the mechanism directly.

2. **Did not help:** I did not filter the `rg` output and cut it with `head`, so some matches were noise (the `stopped` and `sl.cancel` hits). Two `sed` calls read regions I had already seen or didn't need. I never opened `manager.go` around line 559, so the claim about which callers disable end-of-run markers rests on an `rg` hit only. I didn't need the `gh` CLI, since the local checkout was enough.

3. **Next time:** I'd run one `rg` with `-n -C3` on the key symbols. I'd also read `manager.go` and the `sync` code path at the start, so the "target stops being scraped" answer covers the pool and manager side, not just the loop side.

4. **Confidence:** High for the core mechanism, because I read the cited lines directly. Medium for the reload and `disableEndOfRunStalenessMarkers` details, because I didn't read the callers.

## L10

1. **Helped:** My first call combined `git rev-parse HEAD` with an `rg` for `extrapolatedRate|funcRate|funcIncrease|isCounter` in `promql/functions.go`. It confirmed the pinned commit and located the whole implementation at once. The second call, `sed -n 444,640p` plus `sed -n 805,822p`, printed the full function and the `funcRate`/`funcIncrease` wrappers. Two calls were enough.

2. **Did not help:** Nothing errored and I made no repeated calls. The first `rg` output was noisy: it matched many `isCounter` hits in the interpolation and histogram helpers that were not relevant. I used `sed` for file reading, although the instructions said to prefer dedicated tools; the only tool I had was Bash. I did not read `histogramRate`, `extendedRate`, `isStartTimestampReset` or `checkStartTimeOverlap`. My descriptions of them rest on call sites and comments.

3. **Next time:** I would read `histogramRate` and `extendedRate` with one more `sed` if the question needed depth. I would also check the docs or tests, such as `promql/promqltest` testdata, to confirm the behaviour. I would give line citations for the sub-ranges (`:509`, `:519-532`, `:588-590`) only after checking them against the printed output. I cited those from reading the output rather than from numbered lines, so they may be off by a few.

4. **Confidence:** Medium-high on the algorithm, because I saw the code directly. Medium on the exact line numbers. The `sed` output had no line numbers, so I estimated the sub-range citations from the `:452` and `:811` anchors.

## L11

1. **Helped:** The first Bash call did most of the work. It confirmed the checkout HEAD matched the pinned SHA. In the same command, an `rg` for names like `store_output`, `take_output`, `try_read_output`, `wake_join` and `COMPLETE|JOIN_INTEREST` across `core.rs`, `harness.rs` and `state.rs` located the whole mechanism at once. Two `sed` reads followed, and they were enough to trace the producer and consumer paths with line numbers.

2. **Did not help:** The second `sed` pair was partly redundant, since I had already seen `complete` and `can_read_output` in the first read. I also cited some line numbers without seeing them exactly. `state.rs:381` and the `drop_join_handle_slow` range came from grep output and a partial read. I never opened `task/mod.rs`, which holds the safety rules the code comments refer to. I also never traced the vtable wiring from `RawTask` to `Harness::try_read_output`.

3. **Next time:** I would add one `rg` on `try_read_output` in `raw.rs` to confirm the vtable path. I would also read the top of `task/mod.rs`, and use `sed -n` with exact line ranges for every line I cite.

4. **Confidence:** High for the overall mechanism, because I read the code directly. Medium for a few exact line numbers, and for the description of the rules in `task/mod.rs`, which I only saw in comments.

## L12

1. **Helped:** The single `rg -n "lifo|LIFO" worker.rs` call found every relevant site at once. The follow-up `sed` reads of `:115-127`, `:262-270`, `:700-796` and `:1370-1430` gave the actual logic and comments. The `git log -1` call in the same command confirmed the checkout was at the pinned SHA.

2. **Did not help:** The `rg lifo ../../task/mod.rs` call returned nothing, so it was wasted. I also never read the `builder.rs` docs.

   Some citations in my answer were weaker than I implied:
   - **`:1373-1375` (inject queue fallback):** my `sed` output began mid-function, so I only saw the tail. I inferred the "no core" context rather than reading it.
   - **`:479-481` (park/shutdown):** I saw only the grep comment lines, not the surrounding function. "Parks or shuts down" is an inference.
   - **"Unstable option":** I asserted this without seeing the `cfg` or doc text.

   I disclosed only the `builder.rs` gap.

3. **Next time:** After the grep, I'd read `:470-485` and `:1340-1380` and the `builder.rs` docs before citing them. I'd also skip speculative greps like the `task/mod.rs` one.

4. **Confidence:** High for the core mechanism, the cap of 3, the budget check and the yield case, because I read that code directly. Medium for the park/shutdown, inject-queue and unstable-option details, which are lightly inferred.

## L13

**Helped:**
- The first `rg -n` call located `getCommand`, `getGenericCommand` and `expireIfNeeded` in one step. It also confirmed HEAD was 20bb2cfc54 via `git rev-parse`.
- The second call was a `sed -n` over `src/db.c:2940-3110`. It showed `keyIsExpired`, the `expireIfNeeded` doc comment and its full body, which answered most of the question.
- The third call's `sed` on `src/db.c:300-362` showed how `lookupKey` handles `KEY_VALID` and the miss path. Its `rg -A` showed `deleteKeyAndPropagate`.

**Did not help:**
- `sed` output has no line numbers, so I worked out most line citations by counting from the start of each range. Examples are `db.c:3073-3082`, `3050-3053`, `3058-3059`, `3070` and `3010-3019`. They could be off by a few lines. I should have used `rg -n` or `nl`, and I didn't say this in the answer.
- I never read `lookupKeyRead` itself. "No extra flags for a plain GET" is inferred from the flag mapping, though I did note that in the answer.
- I also didn't read `propagateDeletion` or `dbGenericDelete`.

**Next time:** I'd use `rg -n` with context, or `nl -ba | sed -n`, so every cited line is printed. I'd also read `lookupKeyRead` directly.

**Confidence:** High on the behavior, because I read the control flow directly. Medium on the exact line numbers for the derived ranges.

## L14

1. **Helped:** The first call did most of the work. `git rev-parse HEAD` confirmed the pinned commit, and `rg -n "function debounce"` gave the location (`lodash.js:10403`) in one step. The second call printed lines 10403-10525 in full, so I could read the whole implementation at once. The `cat -n | awk` pipeline added line numbers, which let me cite exact lines.

2. **Did not help:** My first `ls debounce.js` failed with "No such file", because this checkout is the monolithic `lodash.js`. That cost a little. The awk numbering left the source without indentation and made it harder to read. I ran no tests, so I never saw the behaviour.

3. **Next time:** I'd skip guessing at a per-function file and go straight to `rg`. I'd use `rg -n` or `sed -n` with `nl -ba`, which keeps indentation. I might also check `test/` for debounce and `maxWait` tests to confirm the behaviour.

4. **Confidence:** High for the mechanics, because I read the source directly at the confirmed commit and cited its line numbers. Medium-high for the summary sentence on `maxWait` guarantees, since that is my inference from the code, not something I ran.

## L15

1. **Helped:** The first Bash call did most of the work. It combined `git rev-parse HEAD`, which confirmed the pinned commit, with one `rg` for flooding-related identifiers (`MAX_RUN_MULTIPLIER`, `maxRunBeforeFallback`, `hashFloodingDetected`, `MAX_HASH_BUCKET_LENGTH`) across four files. That located the mechanism and gave line numbers immediately. The follow-up `sed` calls on lines 655-760 and 795-830 supplied the code and javadoc I cited.

2. **Did not help:** The second `sed`/`rg` call partly repeated the first. My last `sed` (886-935) was needed only for `JdkBackedSetBuilderImpl`, and I could have combined it with the earlier reads. I never read the body of `hashFloodingDetected` (835-870), so the answer says only what its javadoc claims and I flagged that as unverified. I also never opened `RegularImmutableMap`'s `MAX_HASH_BUCKET_LENGTH` use or `JdkBackedImmutableSet`. The `cd` in each command changed the working directory, which was just noise.

3. **Next time:** I'd run one `rg` with context (`-C`) and one `sed` covering 830-885. That would let me verify the detection loop rather than rely on its javadoc.

4. **Confidence:** High for the core claim. The fallback threshold, the `review()` check and the JDK `HashSet` fallback are all in code I read directly. Medium for the details of the full-table scan, since I only saw its comments and a few lines of the method.

## L16

1. **Helped:** My first Bash call did most of the work. It combined `git log -1`, a `find` for `LocalCache.java`, and two `rg` searches on the segment and concurrency terms. That one call gave me the pinned SHA, the file path, and the line numbers for the constructor logic and the `CacheBuilder` getters. The second call was `sed -n 245,325p`, which showed the segment-count loop and the per-segment weight split in full. The same call printed `segmentFor` and `evictEntries`, so I could cite them.

2. **Did not help:** The first `rg` on `CacheBuilder.java` returned a lot of Javadoc lines, but that was minor. I did not check where `getMaximumWeight` is used at line 607, or the `Segment` constructor at line 2000. Because of that I said in the answer that I hadn't traced the table sizing there. I also never ran `git status` to confirm the checkout was clean, so the line numbers rest on the HEAD SHA matching. The `find` might have missed an android variant, but I excluded it on purpose.

3. **Next time:** I would read the `Segment` constructor and the `getMaximumWeight` region in the same `sed` call as the others, so the answer needs no "not traced" caveat.

4. **Confidence:** High. Every claim comes from code I read at the pinned SHA, and it matches the in-source comments. The 4-segment example comes from my own arithmetic, not from running code.

## L17

1. **Helped:** The first `rg` call for `Required.Always|DisallowNull|MissingMemberHandling…` in `JsonSerializerInternalReader.cs` found `EndProcessProperty` (line 2677) straight away. The `sed -n 2660,2725p` read that followed gave the full check logic. The second `rg`/`sed` batch then showed both call sites (lines 2280 and 2588) and where `_required` is set in `DefaultContractResolver.cs` (lines 1519–1587). Guessing the file name from prior knowledge meant I needed no directory exploration.

2. **Did not help:** In my last answer I said `Required.Default`/`DataMember` set `AllowNull` at line 1577 without reading that block. I only saw the grep hit, so I inferred the surrounding context. I never opened `SetPropertyPresence` or `HasRequiredOrDefaultValueProperties`, and I disclosed that gap. The `git log -1` call was redundant because the SHA was already given. The working directory changed after my `cd`, which was harmless.

3. **Next time:** I would run one `rg -n "SetPropertyPresence|HasRequiredOrDefaultValueProperties" -A12` to close the gap on the two unread pieces. I would also `sed` the block around line 1577 to check the `DataMember` claim.

4. **Confidence:** High for the core mechanism, since I read `EndProcessProperty` and both call sites directly. Medium for the `DataMember` detail, which I inferred from a grep hit.

## L18

1. **Helped:** The first Bash call was the most useful. It ran a `grep -n` over `json_sax.hpp` for `keep_stack`, `ref_stack`, `callback` and related terms. That showed the whole callback-parser class (about lines 509-1130) in one pass. The second call then printed the exact regions I needed: `key()`, `handle_value`, `remove_discarded_value`, and `parser::parse`. The third call added `end_object`. I guessed the file location from the nlohmann layout and was right, so I never had to search for it.

2. **Did not help:** The first grep printed a lot of output, and much of it came from the non-callback DOM parser class. I never viewed `end_array` in full. I cited it as "~748-807" from grep hits alone and flagged that in the answer. Also, I never confirmed the commit hash of the checkout. I assumed it was at the pinned commit, as the task said.

3. **Next time:** I would first grep for the class name to get its line range, then read that range once with `sed`. I would also read `end_array` directly, so I wouldn't have to hedge on its line numbers. I would run `git rev-parse HEAD` to confirm the pin.

4. **Confidence:** High for the mechanism and the cited lines, because I read them directly. Medium-high for the `end_array` details, which I inferred from grep output and from its similarity to `end_object`.

## L19

1. **Helped:** The first Bash call was the most useful. It combined `git log -1` (to confirm the pinned SHA) with `rg -n "ModuleDetection"` over the Go files, excluding tests and testdata. That one call surfaced `GetEmitModuleDetectionKind` and `ast/parseoptions.go`. The second call printed the relevant ranges of both files, which gave me the whole decision chain. A third `rg` found the callers (`fileloader.go:422`, `parser.go:486`), which connected the option to the parser.

2. **Did not help:** The second call's `rg` for `ExternalModuleIndicator` was noisy, because checker and transformer hits were irrelevant. I also never opened the `fileloader.go` or `parser.go` call sites. Nor did I check what `GetImpliedNodeFormatForEmitWorker` does, or which module kinds fall in the `Node16..NodeNext` range. The tool environment was fine, with no errors. The only oddity was the working directory changing between calls.

3. **Next time:** I would open `compileroptions.go` around the `ModuleKind` enum to confirm the range. I would also read the `fileloader.go` call site so the parse-options flow is verified rather than inferred from grep hits. I would narrow the `rg` scope from the start.

4. **Confidence:** High. Every claim comes from code I read directly at the pinned commit. The residual uncertainty is the enum range and the implied-format details, which I flagged in the answer.

## L20

1. **Helped:** The first call was `rg -il resolveJsonModule --glob '*.go'`. It showed right away that the Go compiler lives under `tsc/internal`. The second call, `rg -n -i` with the test and generated-file globs excluded, gave every real usage in one pass. The third call read the getter body and the main call sites (`project.go`, `resolver.go`, `tsconfigparsing.go`, `declscompiler.go`). That gave me most of the evidence.

2. **Did not help:** The second `rg` did not exclude `diagnostics/loc/*.generated.json`, so the localized translations flooded the output. I also never looked for where TS5070 and TS5071 are emitted, so that gap is in my answer. I did not read the test files, so the claim that baselines would change is an inference. I did not read `module/util.go:140-175` beyond the grep hit, so my TS7042 description rests on the message name and the `needResolveJsonModule` helper name. I did not verify `fileloader.go:681` beyond the grep line. Its role in loading files is my reading of the field name.

3. **Next time:** I would restrict the search with `--type go` from the start. I would also run one `rg` for `GetEmitModuleKind|GetModuleResolutionKind` and one for `5070|5071` to close the open questions.

4. **Confidence:** Medium-high. The getter and its call sites are confirmed by line numbers I saw. The parts I left open are the TS5070/5071 emission and the effect on tests.
