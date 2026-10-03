# octocode: reflection after 30 questions

# REFLECT.md

Based on 30 research sessions: 10 GitHub-history/remote questions (G01–G10) and 20 local-checkout questions (L01–L20).

## What helped

- **One-shot PR/issue reads with `ghGetHistoryItem`.** A single call with `body`, `changedFiles` and `patches` returned the intent, the file inventory and the diffs together (G01, G02, G03, G04, G08, G09, G10). When the PR number was known, search tools were skipped (G03). `fileFilter` kept large PRs focused (G01, G09). `matchString: "miri"` with `matchContext` pulled just the relevant hunks across a big PR (G10).
- **`ghSearchHistory` by issue number.** Searching the keyword "18837" or "13786" found the fixing PR in one hit (G08, G09).
- **Search first, then batched line-range reads (local).**
  - A `localSearch` for a distinctive identifier or regex alternation gave line anchors and the full call chain. Examples: L02, L03, L04, L05, L06, L07, L08, L09, L10, L11, L12, L15, L16, L17, L19, L20.
  - A batched `localFetch` of several line ranges then returned the deciding code in one round trip.
  - Guessing the file path directly saved calls when the path was known (G07, L01, L14).
- **Regex alternation in `localSearch`** found related definitions in one call (L04, L08, L10, L11, L12, L15, L16).
- **`resultView: "files"`** revealed the repo layout, such as the Go code under `tsc/internal` (L19, L20).
- **`ghGetFileContent` pinned to the SHA** with `matchString` plus `contextLines` was a compact way to read a function (G05, G06, G07).
- **Parallel independent calls** saved round trips (G08, L02, L03).

## What did not help

- **Schema-validation failures were the largest source of wasted calls.** They came up in G05, L01, L02, L03, L04, L05, L06, L07, L08, L09, L10, L11, L12, L13, L15, L16, L17, L19 and L20. The causes:
  - `queries` sent as a string or without the array wrapper.
  - Missing required `goal` / `reasoning`.
  - Arrays (`include`, `exclude`) passed as strings.
  - Numbers or booleans passed as strings (`contextLines`, `pageSize`).
  - More than 5 queries in one call.
  - `contextLines` above the cap of 100.
  - An unsupported `defaultExcludes` parameter.
- **No line numbers in content output.** `localFetch` and `ghGetFileContent` often returned text without per-line numbers, so I counted offsets by hand and marked citations as approximate (L01, L05, L07, L09, L12, L14, L15, G05). Patch views show only hunk headers, so I could not give `path:line` (G03, G04, G08, G09).
- **Elided output.** Patches and `matchString` views hid context with `...` or "lines omitted" (G02, G03, G09, L17). Match views also fragmented across duplicate sections (G06).
- **Windows that cut off mid-function.** I guessed ranges that ended before the point I needed (L04, L05, L07, L11, L18), and fetched non-existent or empty ranges (L12).
- **Response-level pagination hid files.** A batched `ghGetFileContent` returned only the first file, and page 2 was never fetched (G01). Paginated patches and changed-files pages were also left unread (G10).
- **`localFetch` with `matchString` returned `noMatches`** because I had not checked which file held the function (L04).
- **Noisy or low-value output.**
  - The `matchString: "miri"` response was about 20k characters, with irrelevant doc rewordings (G10).
  - The `structureSearch` file listing returned 20 of 5118 files (L20).
  - A duplicate query in a batched search (G09).
  - A filler `startLine: 1` fetch (L12).
- **Truncated search pages.** Results were cut at 10 per page or flagged partial, and I did not read the rest, so completeness was unconfirmed (L06, L08, L16).
- **Coverage gaps.** I did not read the callers, wrappers or tests behind the claims. Many answers therefore rested on inference from names or comments, which I flagged in each.
- **`lspSearch` was unused.** It could have confirmed caller sets semantically (L05, L20).

## Patterns

- **Validation errors cost one call per session.** The pattern is the same everywhere: I skipped the schema and the tool rejected it.
- **Citation quality was the recurring weak point.** Fetch output, patch output and match views often lacked exact line numbers. My answers said "approximately" or "~" and set the confidence to medium.
- **The first pass was strong; the verification pass was skipped.** I read the main mechanism directly, then left secondary claims unverified. Typical examples were the callers, the surrounding branch or tail of a function, the tests, and the post-merge state.
- **Confidence was consistently high on the mechanism** and medium on line numbers and completeness.
- **Repeated fix-ups I named after the fact.** I listed the same follow-up in almost every reflection: add a pinned `ghGetFileContent` or `localSearch` with a `matchString` to get exact lines, and read the neighbouring function.

## Suggested changes

1. **Give the Octocode MCP schemas a strong first-call contract.** Add a short instruction block or worked example per tool showing:
   - `queries` is always an array, at most 5 entries.
   - `goal` and `reasoning` are required on every query.
   - Booleans, numbers and arrays must be typed (`contextLines`, `pageSize`, `include`, `exclude`).
   - `contextLines` is capped at 100.

   Better still, accept and coerce common near-misses (a string that is a valid number or array, or a single query object) instead of rejecting them. This would have removed the most frequent wasted call across 19 sessions.
2. **Return per-line numbers in `localFetch` and `ghGetFileContent` output.** Add a `lineNumbers: true` option or make it the default. This would replace hand-counted, approximate citations with exact `path:line` in most sessions.
3. **Give patch views real line numbers.** Show old and new file line numbers within hunks. Elide unchanged context less aggressively, or offer a "context: N" option (G03, G04, G09).
4. **Add an instruction: verify before citing.** For example: "For each cited line, confirm it with a search anchor or an explicit `startLine`/`endLine` read. If a window ends mid-function, extend it. Read the callers and the neighbouring branch before claiming behaviour."
5. **Fix the pagination affordances.**
   - Make a truncated batched read say plainly which files were omitted and how to fetch them (G01).
   - Show `hasMore` for changed files and patches (G10).
   - Flag when a search is partial, and offer `resultView: "files"` as the standard first pass for completeness (L06, L08).
6. **Add a "locate then read" habit to the instructions.**
   - Run a regex `localSearch` with `def`/`func` to find definitions before guessing a file for `matchString` (L04).
   - Read the full function range, not a guessed window (L18).
   - Prefer `lspSearch` references/callers when the question is about who uses something (L05, L20).
7. **Reduce noise in broad queries.** Let `matchString` on patches take a `fileFilter` by default, or support path exclusion. Make `structureSearch` return a full tree count or a clear "partial" signal instead of an arbitrary 20 of 5118 (G10, L20).
8. **Have the harness carry over a valid template.** Since every session repeats the same schema mistakes, ship a tested example call for each tool in the tool descriptions.

---

# Per-question reflections

## G01

1. **Helped:** The first `ghGetHistoryItem` call (body plus `changedFiles`) gave the merge SHA, the file inventory and the size in one shot. The second `ghGetHistoryItem` call, with `fileFilter` on four paths and `patches: all`, returned the diffs I needed for `applications.py`, `routing.py` and `background.py`. I did not have to page through 36 files. `ghGetFileContent` on `_runtime.py` at the merge SHA gave the environment auto-configuration logic with line-level detail.

2. **Did not help:** I batched three files in one `ghGetFileContent` call. The response was paginated at the response level, so only `_runtime.py` came back. `_api.py` and the docs were left on page 2, and I never fetched it. I also never read `_asgi.py`, the tests or the rest of the `pyproject.toml` patch, which was truncated. My answer flagged these gaps. Its claims about span contents and data exposure remain unverified.

3. **Next time:** Fetch the big files one per call, or set `responseCharLength` high enough for all three. Continue the `next` pagination when it is offered. Use `matchString` on the patch for `attributes` or `body` to check for PII exposure. Read `_asgi.py` with `minify: symbols` first.

4. **Confidence:** Medium. The routing, lifespan and dependency claims are grounded in diffs I read. The telemetry contents and the double-instrumentation behavior are inferred, not verified.

## G02

1. **Helped:**
   - The first `ghGetHistoryItem` call (PR summary plus `changedFiles`) mapped the 31 files at once and gave me the merge SHA.
   - The batched `ghGetFileContent` read of `validators/counter.rs` and `tests/validators/test_counter.py` at the merge SHA gave the validator's behavior directly. The tests worked as a clear behavior spec.
   - The selected-patches `ghGetHistoryItem` call showed the before/after in `_generate_schema.py` and `_validators.py`. It also returned `tests/types/test_counter.py`.

2. **Did not help:**
   - The patch output elided hunks with `...`, which cut some context, including part of the old test code.
   - I never saw the pre-PR `_mapping_schema` or the old constraint behavior. The "before" side is inferred from removed diff lines and the PR body.
   - I did not read `input_python.rs`, so the strict and lax rules come from the tests, not the implementation.

3. **Next time:**
   - Read `_mapping_schema` at the parent commit with `ghGetFileContent`.
   - Fetch the `input_python.rs` patch for `validate_counter`.
   - Check issue #13704 with `ghGetHistoryItem` to confirm what constraint behavior was broken.

4. **Confidence:** medium-high on the new behavior, because the source and tests were read directly. Medium on the comparison with the old behavior, because it is inferred.

## G03

1. **Helped:** One call did the work: `ghGetHistoryItem` (operation pullRequest, #5881) with `content: {body, changedFiles, patches: {mode: "all"}}`. It returned the PR body, all four file patches and the new test file together. The patch comments in `pool-base.js`, `pool.js` and `round-robin-pool.js` explained the cause and the fix. Because the PR number was known, I skipped search tools.

2. **Did not help:** Nothing failed and I made no wasted calls. The patch output collapsed unchanged context to `...`, so I couldn't see how `kOnDrain` and `kDestroy` fit around the edits. I stated the `kOnDrain` behavior only as the patch comment says it. The PR body's "Bug Fixes" section was N/A, so the description gave only the symptoms.

3. **Next time:** Add a `ghGetFileContent` read of `lib/dispatcher/pool-base.js` at the merge commit. Use `matchString` for `kOnDrain` and `kRetiring`. That would confirm the drain claim directly and give real line numbers. My answer cited files and patch content but no `path:line` references, because the diff hunks didn't give reliable ones.

4. **Confidence:** Medium-high. The mechanism and fix come straight from the diff and its comments. The `kOnDrain` claim and exact lines are unverified.

## G04

1. **Helped:** Two `ghGetHistoryItem` calls did all the work. The first was a PR summary with `body` and `changedFiles`. It gave the title, the intent (identifier, keyword and lower-case checks) and the file inventory. The second asked for selected patches of `src/click/core.py` and `CHANGES.md` with `minify:"none"`. It returned the exact warning conditions, messages and call sites, so no source reads were needed.

2. **Did not help:** Nothing errored and no calls were wasted. The patch view has no source line numbers, only hunk headers. I couldn't give `path:line` citations. I also didn't read the merged file with `ghGetFileContent` at `mergeCommitSha`. I skipped the test and docs patches, so my description of them rests on the changed-file counts. I also didn't check `Parameter` subclass behaviour.

3. **Next time:** After the patch, I'd run `ghGetFileContent` on `core.py` at `06b2a678741131fd577ce170e23e5ca0aeba0309` with `matchString` `_check_name_is_usable`. That would give real line numbers and let me confirm the merged code matches the PR head diff. I'd also patch-read `test_deprecations.py` to check the exact parameter declarations that warn.

4. **Confidence:** Medium-high. The PR body, the changelog and the diff agree on the behaviour. The gaps are the missing line citations and the untested subclass case.

## G05

1. **Helped:** `ghGetFileContent` on `src/requests/sessions.py` at the pinned SHA. Using `matchString: "def resolve_redirects"` with `contextLines: 100` returned most of the loop in one call. A second call with `startLine` 286 to 400 covered the loop's end, `rebuild_auth`, `rebuild_proxies` and `rebuild_method`. Two reads were enough. I skipped search and structure calls because I already knew the file.

2. **Did not help:** three of my calls failed validation. I passed `queries` as a string instead of an array, `contextLines` was capped at 100, and `goal` and `reasoning` were required. Each cost a round trip. The first content chunk carried no per-line numbers, only `sourceLineRanges` and one `matchedLines` entry (186). So my line citations are counted or estimated, and I said so in the answer. I never read where `Session.send` calls `resolve_redirects`, or where `allow_redirects` is handled.

3. **Next time:** read the tool schemas first and send valid queries from the start. Ask for explicit `startLine` and `endLine` ranges so the returned line numbers can be cited exactly. Add one `matchString` read on `resolve_redirects(` in `Session.send` to close the gap on `allow_redirects`.

4. **Confidence:** high on the behaviour. I read the code directly at the pinned commit, and every listed change comes from those lines. Medium on the exact line numbers, since most are approximate.

## G06

1. **Helped:** The first `ghGetFileContent` call used `matchString: "proxy"` with `contextLines: 0`. It returned a compact map of the matching lines. That showed me where `_get_proxy_map`, the `_mounts` construction and `_init_proxy_transport` sit. It gave me the line numbers for the second call. The second `ghGetFileContent` call was a batch of two `startLine`/`endLine` ranges (238–254 and 684–800). It returned the deciding code, including `_transport_for_url`, in one round trip. I passed the full SHA as `branch`, so the reads were pinned to the commit.

2. **Did not help:** The first call's output was fragmented. It had many "lines omitted" gaps, and it also matched the async client's duplicate sections. I had to make a second call anyway, because the match view never showed `_transport_for_url`, whose body doesn't contain "proxy". I didn't read `URLPattern.matches` (in `_utils.py`), `get_environment_proxies`, or the callers of `_transport_for_url`. Because of that, the sort order and the `no_proxy` handling are stated as unverified in my answer.

3. **Next time:** I would batch a `matchString` read and the ranges together. I would also search for `_transport_for_url` and `class URLPattern` directly. Then I could confirm the matching and ordering rules in the same round.

4. **Confidence:** High for the core flow: proxy map, then `_mounts`, then first-match in `_transport_for_url`. I read that code directly at the pinned commit. Medium for the environment-proxy and ordering details, which I didn't read.

## G07

1. **Helped:** I made two `ghGetFileContent` calls, both pinned to branch `63c5760d8a`. The first used `matchString: "def build_middleware_stack"` with `contextLines: 40`. It returned `__init__`, the build method and `__call__` in one shot. The second fetched `starlette/middleware/exceptions.py` and `starlette/_exception_handler.py` with `fullContent`. Together these covered both halves of the question. Each result included `commitSha`, which confirmed the pin. Guessing the file paths from my knowledge of Starlette skipped discovery calls.

2. **Did not help:** Nothing errored and I made no redundant calls. Line numbers weren't reported for the matched-context view, so I counted them by hand from `sourceLineRanges`. I never read `ServerErrorMiddleware` or `RequestBodyLimitMiddleware`, and I didn't open the router to check where else `wrap_app_handling_exceptions` is used. I marked those points as unverified in the answer.

3. **Next time:** I would add a `ghGetFileContent` read of `starlette/middleware/errors.py` and a `ghSearchCode` for `wrap_app_handling_exceptions` in `routing.py`. That would let me confirm the outermost-handler behavior and the per-route usage instead of inferring them.

4. **Confidence:** High for the stack order and the handler-lookup logic, because I read that code directly. Medium for the `ServerErrorMiddleware` behavior and the routing usage, which I inferred and flagged as such.

## G08

1. **Helped:** Two parallel calls did nearly all the work. `ghGetHistoryItem` (issue #18837, with body and comments) gave the bug report. `ghSearchHistory` (pullRequest, keyword "18837") found PR #18838 in one hit. The follow-up `ghGetHistoryItem` on #18838 with `patches: {mode: "all"}` returned the whole diff, including the fix, the test and the changeset, plus a PR body that states the intent. The PR was small, so a single unfiltered patch read was cheap.

2. **Did not help:** Nothing errored and I made no redundant calls. The issue had no comments, so the discussion fetch added nothing. I never opened `proxy.js` with `ghGetFileContent`. I therefore have no pinned-commit line numbers, only diff hunk offsets. I also never read the `has` trap, so my statement that the fix reuses its eligibility logic rests only on the `this.has?.()` call in the diff and the PR body. The PR body's test counts are self-reported and I couldn't check them.

3. **Next time:** I'd add a `ghGetFileContent` read of `proxy.js` at the merge commit or `sourceSha`. It would give me the `has` trap and exact `path:line` citations. I'd also check the PR's linked issue comments and reviews for maintainer discussion of the root cause.

4. **Confidence:** medium-high. The mechanism follows directly from the diff and the issue title, but the `has` trap semantics and line numbers are unverified.

## G09

**1. Helped:** Three calls did the work.
- `ghGetHistoryItem` (issue #13786, body and comments) gave the root cause directly. The reporter had already diagnosed the `core_config()` mutation.
- `ghSearchHistory` with keyword "13786" over PR title and body surfaced #13825, plus the closed candidates #13787 and #13794, in one shot.
- `ghGetHistoryItem` on #13825 with `patches: all` and a `fileFilter` on `pydantic/**` showed the whole fix. The filter kept the output focused.

**2. Did not help:**
- The second query in the batched `ghSearchHistory` call repeated what the first already found.
- The patch output was minified with `...` elisions. I saw the diff but not the final source with line numbers, so I couldn't cite `path:line` at the merge commit.
- I never read the tests, so I couldn't say how the fix was verified.
- I never checked the merge commit SHA or the release it shipped in.
- Three bot comments were hidden, and I didn't check whether they added anything.

**3. Next time:**
- Follow up with `ghGetFileContent` at the merge SHA for `_build_effective_config` to get exact line citations.
- Include `tests/**` in the `fileFilter` to see the regression test.
- Read the PR's non-bot discussion comments.

**4. Confidence:** High on root cause and fix, because the issue text and the merged diff agree. Medium on completeness, since I saw no tests or line-level citations.

## G10

1. **Helped:** The first `ghGetHistoryItem` call (PR body plus `changedFiles`) gave the motivation and the two open Miri issues, #5047 and #5054, in one shot. The second call used `matchString: "miri"` with `matchContext: 1`. It returned every Miri-related hunk across the PR, so I didn't have to page through files. The third call, with `files` selected, filled in the paginated `tcp_*` patches. I found the "why" for each ignore in the `ignore = "..."` strings and comments.

2. **Did not help:** The second call's output was large, about 20k characters. It included UDP and Unix-socket doc-comment rewordings that didn't matter for the question. Several `tcp_*` patches came back truncated ("hasMore"), so the third call was needed. I never fetched changed-files page 2, so my file list may be incomplete. The tool also can't show the post-merge state of the files. I couldn't confirm what remains ignored beyond what the diff showed. I also didn't check whether Miri issues #5047 and #5054 have since been fixed.

3. **Next time:** I'd run the `matchString` query with `fileFilter` for `tokio/tests/**`, plus a narrower one for `ignore`. I'd also fetch the changed-files page 2. If I needed the current state, I'd use `ghSearchCode` or `ghGetFileContent` at the merge SHA.

4. **Confidence:** Medium-high. The listed ignores and reasons come straight from the diff. The completeness claim is weaker, since I never read the second changed-files page and the current state of Miri #5047 and #5054 is unknown.

## L01

**1. Helped:** The third call, `localFetch` on `django/middleware/common.py` with `startLine` 1 to 180, gave me everything in one read. It returned `CommonMiddleware`, `should_redirect_with_slash`, `get_full_path_with_slash` and `process_response` together. Guessing the file path directly, without searching first, saved calls.

**2. Did not help:** My first two calls failed schema validation. `localSearch` needed `include` as an array, and `localFetch` needed `goal` and `reasoning`. That cost two round trips, and the `localSearch` call was never re-run. The `localFetch` output has no line numbers, so I counted lines by hand from the raw text. I therefore hedged the line citations as approximate (±2), which is weaker than the evidence format asks for. I never opened `global_settings.py`, `is_valid_path` or the `should_append_slash` decorator. I only said so in the uncertainty note.

**3. Next time:** I would include `goal` and `reasoning` on every query from the start. To get exact line numbers, I would use `localSearch` on `should_redirect_with_slash`, which returns line anchors, or fetch small ranges. I would also batch a read of the `APPEND_SLASH` default and of `is_valid_path` in `django/urls/base.py` into the same call.

**4. Confidence:** High on the mechanism, because I read the code directly. Medium on the exact line numbers. The `APPEND_SLASH` default and `is_valid_path` internals are unverified.

## L02

1. **Helped:** The `localSearch` for the literal "prohibited to prevent data loss" gave me `base.py:1301` and `:1328` in one call. A `localFetch` of lines 1280–1345 then showed the whole check body. A second `localSearch` for `_prepare_related_fields_for_save` listed the call sites (`base.py:864`, `query.py:794`, `query.py:1042`). I ran that search in parallel with the fetch, which saved a round trip.

2. **Did not help:** My first `localSearch` failed validation because I passed `contextLines` as a string. Passing `queries` as a JSON string rather than an array made this easy to get wrong. I never read the method header (1276–1279), so I don't know the `fields` default or its docstring. I only saw the opening of the `query.py:1042` call, so I couldn't say which operation it belongs to. I also didn't confirm that `_is_pk_set` is what I described, or check the pinned commit. The user gave the local checkout, and I assumed it was at that commit without verifying.

3. **Next time:** I'd fetch a slightly wider range (1270–1335) to include the signature. I'd read one line of context around `query.py:1042` to name its operation. I'd read `_is_pk_set` too.

4. **Confidence:** High for the location and mechanism, since I read that code directly. Medium for completeness, because of the unread call site and the unchecked commit.

## L03

1. **Helped:** The first parallel `localSearch` calls, for `run_on_commit` and `def on_commit`, gave every line anchor in one round: the storage, reset and run lines in `base.py`, and the wrapper in `transaction.py`. Batched `localFetch` line-range reads then returned the exact bodies. The `localSearch` for `run_commit_hooks_on_set_autocommit_on` (with `contextLines: 2`) showed how the hooks get triggered.

2. **Did not help:** I wasted two `localFetch` calls on validation errors. The first omitted the required `goal` and `reasoning` fields. The second passed six queries against a limit of five. I did not read `atomic.__exit__` in `transaction.py`, so the link from the outer atomic block to `commit()` and the autocommit restore is inferred. I also did not read `commit()` itself (around line 320), only the `run_commit_hooks_on_set_autocommit_on = True` line. My update to the user came after several silent calls.

3. **Next time:** I would fill in every required field and stay within five queries per call. I would also add one `localSearch` or `localFetch` for `atomic.__exit__` and `commit()` in the first batch, so the full chain is verified rather than partly inferred.

4. **Confidence:** High for storage, discard and run behaviour, because I read each of those lines directly. Medium for the atomic-exit trigger, since I did not read that code.

## L04

**Helped:** `localSearch` for `parse_docstring` mapped the call chain (convert.py, structured.py, base.py) in one call. The batched `localFetch` of base.py 80-360, structured.py 190-300 and convert.py 300-345 then gave most of the logic in one round trip. A regex `localSearch` (`def _parse_google_docstring|def _create_subset_model\b`) found both helper definitions after my literal `matchString` fetch had returned `noMatches`.

**Did not help:**
- My first `localSearch` failed validation because I left out the required `goal` and `reasoning`. That cost one call.
- The `localFetch` with `matchString: "def _parse_google_docstring"` returned nothing because the function is in `utils/function_calling.py`, not base.py. I had not checked where it was defined before guessing the file.
- The fetch windows ended mid-function, at `function_calling.py:800` and `pydantic.py:345`. I never read the end of the `Args:` parsing loop or the body of `_create_subset_model_v2`. Because of that, the answer says the parsed description reaches `__doc__` via inference, not from a line I read.
- My `~` line numbers were estimates from window offsets, not exact anchors. That is weak citation practice.

**Next time:** locate the definitions first, using a regex `localSearch` with `def`. Then read exact ranges that cover each whole function. That would have included the v2 subset model and let me cite exact lines.

**Confidence:** medium-high. The call chain and parser behavior were read directly. The step where the description is written onto the schema was not verified.

## L05

1. **Helped:** The first `localSearch` for `merge_content` under `libs/core` listed every definition, caller, export and test in one call. The batched `localFetch` (base.py 364-460, ai.py 655-670, test_messages.py 1070-1112) then gave the signature and the variadic call sites. Sending those reads in parallel saved round trips.

2. **Did not help:**
   - My first `localFetch` batch failed validation. I sent three bare calls without the `queries` wrapper, and the error message was clear.
   - The `base.py` window ended mid-statement at line 460, so I never saw the tail of the list branch of `__add__`.
   - I never opened `chat.py`, `function.py` or `tool.py`. I judged those call sites from the search snippets alone.
   - I used no `lspSearch` references, so I could not confirm the caller set beyond text matches or search outside `libs/core`.

3. **Next time:** Use `lspSearch` `references` or `callers` on `merge_content` for a semantic caller list. Search the whole repo, not just `libs/core`. Read the ranges I skipped.

4. **Confidence:** Medium-high on the two variadic call sites (`base.py:453`, `ai.py:665`) and the test at `test_messages.py:1109`, because I read those lines directly. Medium on the "unaffected" callers, since I judged them only from search snippets. Low on anything outside `libs/core`, which I did not search.

One correction to my answer: I wrote that `ai.py:667` is a "sibling" `merge_dicts` call. I only saw the `merge_dicts` calls at `ai.py:666-669`, so the exact line number is unverified.

## L06

1. **Helped:** The first `localSearch` for `generateEtags` under `packages/next/src` gave the whole call chain in one call. It listed `config-shared`, `base-server`, `send-payload`, `router-server` and `pages-handler`. The batched `localFetch` of four line ranges then gave the deciding code in one round trip, including the `generateEtags && payload !== null` check in `send-payload.ts` and the `etag: config.generateEtags` option in `router-server.ts`.

2. **Did not help:** Two calls failed schema validation. `localFetch` needed `goal` and `reasoning` on every row and rejected `defaultExcludes`. `localSearch` rejected `pageSize` as a string. The first search returned `isPartial` / `terminalLimit`, so I couldn't be sure the usage list was complete. The re-run I meant to do with `resultView: "files"` never went through, so I never confirmed the list. I also never opened `generateETag`, `sendEtagResponse` or `serveStatic`, so parts of my answer rest on inference from names and comments.

3. **Next time:** Fill in every required field correctly on the first try. Use `resultView: "files"` first to get a complete file list. Read `generateETag` and `sendEtagResponse`, and check the app-router path. Use `lspSearch` for definitions instead of guessing what a name does.

4. **Confidence:** Medium-high on the main flow, because I read those lines directly. Medium on completeness and on what `sendEtagResponse` and `serveStatic` do.

## L07

1. **Helped:** The `localSearch` for `getURLFromRedirectError` (with `maxMatchesPerFile`) was the fastest step. It listed every consumer in one call: app-render, action-handler, app-route module, and make-get-server-inserted-html. The `localFetch` full read of `redirect.ts` gave the digest format and the helper functions. The batched `localFetch` line-range reads of `app-render.tsx` and `module.ts` showed where the status code and `Location` header are set.

2. **Did not help:** My first two calls failed validation because I left out the `queries` wrapper and passed a string for a boolean. That cost one round trip. The `module.ts` read ended mid-expression at `status: actionStore.isAction`, so I never saw the actual status values. I also did not open `action-handler.ts:1325`, `make-get-server-inserted-html.tsx`, or the `RedirectStatusCode` enum. My answer reports the numeric codes only from doc comments, though it says so.

3. **Next time:** I'd get the queries schema right on the first call. I'd read `module.ts` a few lines further, and open `redirect-status-code.ts` and the action-handler region in the same batch. I'd also add a `localSearch` for `RedirectStatusCode` to confirm the numeric values.

4. **Confidence:** Medium-high on the core mechanism (throw an error with a digest, catch it, set the status code and `Location` header), because I read that source directly. Medium on the server-action and meta-tag details, which I inferred without reading.

## L08

1. **Helped:** The `localSearch` regex query `sampleLimit|errSampleLimit|ErrSampleLimit` over `scrape/` returned line anchors in `scrape.go`, `scrape_append_v2.go` and `target.go`. That gave me the whole enforcement path in one call. The batched `localFetch` of four ranges then gave the deciding source: `limitAppender` in `target.go`, `appenderWithLimits`, and `checkAddError`. Batching kept it to two productive calls.

2. **Did not help:** Both first attempts failed schema validation. `localSearch` was rejected because I passed `exclude` and `contextLines` as strings, not as an array and a number. `localFetch` was rejected because `goal` and `reasoning` are required per query. Each cost a round trip. The search also had a second page of 16 matches that I never read. I skipped where `sl.sampleLimit` is set from the job config, and I never read the code after the loop that decides whether the scrape's samples are discarded. My answer says both are unverified, but a couple more fetches would have closed them.

3. **Next time:** I'd read the tool schemas before the first call. I'd add a search for `sampleLimit` in `newScrapeLoop` and the config wiring. I'd also read the rollback/commit code after the `sampleLimitErr` handling.

4. **Confidence:** High for the enforcement mechanism, because I read the cited lines directly. Medium for completeness, because of the two gaps above.

## L09

1. **Helped:** The first successful `localSearch` for `StaleNaN` with `include`/`exclude` globs gave a quick map of the codebase. It pointed straight at `scrape/scrape.go`. The `localFetch` line-range reads of `updateStaleMarkers`, `endOfRunStaleness` and `forEachStale` gave the core evidence. The `matchString` regex read showed every call site in a single outline.

2. **Did not help:**
- My first `localSearch` failed because I passed `include`/`exclude` as strings, not arrays.
- My first `localFetch` batch failed because it lacked the required `goal`/`reasoning`.
- A `matchString` regex with an unbalanced `(` errored.
- The `matchString` output showed line numbers that did not line up with my requested ranges. My cited line numbers ("about 1754", "about 2082") are therefore approximate.
- I never read `scrapeAndReport`, so I couldn't confirm that a failed scrape reaches the empty-append path. I flagged that in the answer.
- I also never checked all callers of `scrapePool.disableEndOfRunStalenessMarkers`.

3. **Next time:** Read schemas before the first call. Use `localSearch` with simple literals and a few lines of context, so I get exact line numbers instead of outline-style output. Read `scrapeAndReport` directly.

4. **Confidence:** Medium-high on the mechanism, because I read the code. Medium on exact line numbers and the failed-scrape path.

## L10

1. **Helped:** The `localSearch` for `func extrapolatedRate|func funcRate|func funcIncrease` in `promql/` gave exact line anchors on the first successful try. The batched `localFetch` (lines 435-720 plus 808-822) then returned the whole implementation and the wrappers in one round trip. That was enough to answer the question.

2. **Did not help:** My first `localSearch` failed validation because I left out the required `goal` and `reasoning` fields. That cost one wasted call. I also did not read `extendedRate` or `extendedHistogramRate`. I did not identify the helper ending near line 435, or check whether `funcRate` has another wrapper for histograms. I gave an approximate line for the `resultFloat` subtraction ("~line 508") instead of a checked one. I ran no LSP or `astSearch` step to confirm the call graph, and I did not check the tests.

3. **Next time:** I would fill in the `goal` and `reasoning` fields on every call. I would use `localFetch` with `matchString` on `last.F - first.F` to get an exact citation line. I would also read `extendedRate` when the question could involve smoothed or anchored selectors.

4. **Confidence:** Medium-high. The core algorithm (reset correction, 1.1× extrapolation threshold, zero-point cap, division by the range for `rate`) comes straight from the source I read. The peripheral claims (line numbers and the smoothed/anchored path) are less certain, and I flagged them as unverified.

## L11

1. **Helped:** The first successful `localSearch` (one regex alternation over `runtime/task`) located `store_output`, `take_output`, `try_read_output`, `complete` and `set_join_waker` in one call. The batched `localFetch` with five line ranges then gave me the write, read and wake paths in a single round trip. The last `localSearch` (restricted to `harness.rs`, `join.rs` and `core.rs` by `include`) gave me the exact line numbers for the citations.

2. **Did not help:** My first `localSearch` failed validation because I left out `goal` and `reasoning`. That cost one wasted call. The `localFetch` of `join.rs` 300-345 cut off just before the `try_read_output` call. I needed the extra search to confirm line 346. I never opened `state.rs` or `raw.rs`, and I never opened `harness.rs` around lines 500-560. I therefore did not read `poll_future` itself. My claim about `harness.rs:551` and `harness.rs:508` rests only on grep hits for `store_output`.

3. **Next time:** Include `goal` and `reasoning` on every query. Fetch slightly wider ranges. Read `state.rs` `transition_to_complete` and the `poll_future` context. Then the synchronization claims would be verified rather than partly inferred. I would also skim the ownership rules in the `task/mod.rs` header.

4. **Confidence:** Medium-high. The core mechanism is confirmed from the source I read (stage cell, join waker, `take_output`). The `state.rs` details and the `poll_future` context are unverified.

## L12

**1. Helped:**
- The first `localSearch` for "lifo" in `multi_thread/` returned every relevant anchor at once: the constant at `:269`, the slot take at `:727` and the disable at `:764`.
- The batched `localFetch` of `:695-810` returned the whole run loop, including the poll cap, the budget check and the core-stolen break.
- The `localSearch` with a regex alternation on `worker.rs` pointed me to `schedule_local` at `:1392`. The `localFetch` of `:1340-1440` then gave me the push logic and the notify rule.

**2. Did not help:**
- My first `localSearch` failed validation because I omitted `goal` and `reasoning`. That cost one call.
- I put a pointless `startLine: 1` fetch in the batched `localFetch`. It returned one line.
- `localFetch` returned line ranges but didn't prefix each line with its number. I had to count offsets to get citations like `:757-762` and `:1398-1408`, which is error-prone.

**3. Next time:**
- Include `goal` and `reasoning` on every query from the start.
- Skip filler queries.
- Read `next_task` and the stealing code, which I skipped.

**4. Confidence:** medium-high.
- The mechanism and the limits come from code I read directly.
- The exact line citations are hand-counted and may be off by a line or two.
- "The slot isn't stealable" rests only on the comment at `:478-480`, not on the steal code.

## L13

1. **Helped:** The first batched `localSearch` (two queries, with `contextLines` and `include` globs) found `getGenericCommand` and `expireIfNeeded` at once. It also returned enough surrounding code to show the GET path. The `localFetch` batch then gave the full `expireIfNeeded` body and the `lookupKey` doc comment and code with only one `matchString`.

2. **Did not help:**
   - The first `localFetch` failed validation because I left out `reasoning`. That cost one wasted call.
   - `localSearch` returned line numbers for the match window rather than for the specific function. The `t_string.c` numbers were therefore imprecise, and I had to say "approximately" in the answer.
   - I never opened `lookupKeyReadOrReply`, `keyIsExpired` or `deleteExpiredKeyAndPropagate`. The GET-to-`lookupKeyRead` link is inferred, not verified.
   - The `lookupKey` line numbers were also not confirmed. The `matchString` fetch showed a source range of 206–386, and I didn't pin exact lines from it.

3. **Next time:** I would include `reasoning` and `goal` in every query from the start. I'd add a `lookupKeyReadOrReply` query to the first batch. I'd also fetch `t_string.c` with a `matchString` on `getGenericCommand` to get exact line numbers, or use `lspSearch` on the symbol.

4. **Confidence:** medium-high on the behavior, since the `expireIfNeeded` and `lookupKey` code I read supports it directly. Medium on the exact citations, since some line numbers are approximate or missing.

## L14

1. **Helped:** Two calls did the work. `localSearch` for `function debounce`, restricted to `lodash.js`, returned line 10403 immediately. Then one `localFetch` with `startLine`/`endLine` 10403–10525 returned the whole implementation. No other calls were needed.

2. **Did not help:** Nothing failed and I made no wasted calls. The `localFetch` output had no line numbers, so I counted offsets by hand from 10403 to cite line numbers. That is why I flagged them as possibly off by a line or two. I should have avoided that: I could have run a `localSearch` with a `matchString` for `shouldInvoke`, `leadingEdge` and similar, which returns exact line anchors. I never checked `debounce.js` or the docs, and I didn't read the tests or history. I also didn't verify that this file is the only definition. The `include` filter covered both `debounce.js` and `lodash.js`, but only `lodash.js` matched.

3. **Next time:** Add one anchor-returning `localSearch` for the key function names, so the citations are exact instead of computed by hand.

4. **Confidence:** High on the behavior. It comes straight from the fetched source, which showed `shouldInvoke`, `remainingWait`, and the `maxing` branches in `debounced`. Medium on the exact line numbers for the individual helper functions.

## L15

1. **Helped:** The first `localSearch` was the fastest step. It used an identifier regex (`hashFloodingDetected|MAX_RUN...`) restricted to a few files. That found `ImmutableSet.java` and the key method names in one call. The `localFetch` of lines 640–960 then gave the whole mechanism: `insertInHashTable`, `maxRunBeforeFallback`, `hashFloodingDetected` and `JdkBackedSetBuilderImpl`. The last `localSearch` added the map and multiset analogues and two confirmed line anchors (`ImmutableSet.java:725`, `:744`).

2. **Did not help:** My first `localFetch` failed validation because I left out `goal` and `reasoning`, so I had to repeat it. The fetched content had no line numbers. I had to infer the lines for `insertInHashTable` and other locations by counting, and I flagged those as approximate. I also never read the map and multiset overflow behavior.

3. **Next time:** Include `goal` and `reasoning` on every call. Run a `localSearch` on `insertInHashTable` and `MAX_RUN_MULTIPLIER` alongside the first search, so every cited line comes from a search hit. Read `RegularImmutableMap` around line 250 if the map case matters.

4. **Confidence:** High for the core answer (`ImmutableSet` falls back to a `HashSet`-backed set when it detects long probe runs), because I read the code directly. Medium for the exact line numbers of some symbols, and for the map and multiset details, which I only saw as constants.

## L16

**Helped:** The first `localSearch` call, with a regex over `segmentShift|segmentMask|segmentCount|concurrencyLevel` on `LocalCache.java`, found the constructor logic at lines 250 and 285-293 in one step. The batched `localFetch` of lines 245-325 and 1960-2005 then gave the sizing loop, the per-segment weight split and `initTable`. The `localSearch` on `CacheBuilder.java` gave the line numbers for the defaults and the declarations.

**Did not help:** My first `localSearch` call failed validation because I left out the required `goal` and `reasoning`, which wasted a call. The search output truncated matches at 10 per page with "moreLines" hints. I did not need the rest, but it hid `MAX_SEGMENTS` and the other occurrences.

**Next time:** I would include `goal` and `reasoning` in every query from the start. I would also fetch the `MAX_SEGMENTS` definition and the bodies of `CacheBuilder.maximumSize` and `concurrencyLevel`. I skipped both, so my answer names the cap but not its value, and cites those two methods only by declaration line.

**Confidence:** High for the segment sizing and eviction split, because I read the source directly and cited lines. Medium-high for the `initTable` threshold detail, which I cited from the `Segment` range I fetched. I did not verify the `MAX_SEGMENTS` value and said so.

## L17

**1. Helped:** The first successful `localSearch` on `JsonSerializerInternalReader.cs` was the fastest step. One regex on `Required\.|HasRequired...` returned the tracking condition (lines 2065 and 2467) and the throw sites (2677–2705). The `localFetch` on lines 2660–2725 then gave the full `EndProcessProperty` logic. The `matchString` query for `EndProcessProperty(` showed the call sites. Its output elided most lines, but the sites at 2280 and 2588 were visible.

**2. Did not help:**
- Both my first `localSearch` and my first `localFetch` failed schema validation. I had passed `contextLines` as a string, and I had left out the required `goal` and `reasoning` fields. That wasted two calls.
- The `matchString` fetch printed "lines omitted" gaps, so I could not see the code around the call sites.
- One `localSearch` value was truncated at 200 characters.

**3. Next time:**
- Include `goal` and `reasoning` on every call, and pass numbers as numbers.
- Read the `PropertyPresence` assignment near line 2467 with a targeted `localFetch`.
- Check how `HasRequiredOrDefaultValueProperties` is computed in `JsonObjectContract`. I skipped both, so the answer has gaps there.

**4. Confidence:** High for the enforcement logic and the error messages, because I read those lines directly. Medium for the overall picture, since I did not read where presence is recorded or where `HasRequiredOrDefaultValueProperties` is set. The answer says so.

## L18

**Helped:** The first `localSearch` for `keep_stack` in `json_sax.hpp` found the callback parser class immediately. That is what let me skip browsing the tree. The batched `localFetch` of lines 500-720 and 990-1130 returned the core logic: `start_object`, `key`, `end_object` and `handle_value`. The `localSearch` for "discarded" in `parser.hpp` gave the top-level discard-to-null behaviour and its line numbers.

**Did not help:** My line ranges were guesses, so I never saw `end_array`, `remove_discarded_value` or the body of `parse()` in `parser.hpp`. The `localSearch` hits in `parser.hpp` were only match lines with no context. I cited `:128` and `:130` from those lines and did not read the code around them. I marked the ranges for `key()` and `end_object` as approximate in the answer because I never pinned them down. No tool errors occurred.

**Next time:** I would batch a `localFetch` with `matchString` for `end_array` and `remove_discarded_value` in the same call as the first read. I would also read `parser.hpp` around lines 110-135 directly. I could also have used `astSearch` symbols on `json_sax.hpp` to get exact function line numbers.

**Confidence:** Medium-high. The main mechanism is directly supported by code I read. The array-end behaviour and the exact ranges for `key()` and `end_object` are inferred or approximate.

## L19

1. **Helped:** The third `localSearch` call, run with `resultView: "files"` and no path guess, gave me the list of Go files that mention `ModuleDetection`. It also showed that the Go code lives under `tsc/internal`, not `internal`. The batched `localSearch` on `parseoptions.go` and `compileroptions.go` then gave me the line anchors. One batched `localFetch` of both line ranges returned all the decision logic, which was enough to answer.

2. **Did not help:** The first `localSearch` failed validation because I passed `include`, `exclude` and `excludeDir` as strings when the tool wants arrays. The second call failed because I guessed the path `typescript/internal`. That was two wasted calls. `localFetch` returned file-relative content, so I could only confirm some line numbers from the earlier search anchors. Lines around 40 and after 125 I estimated, and I said so in the answer. I never read the call sites of `GetExternalModuleIndicatorOptions` or the JSX-tag walker `walkTreeForJSXTags`.

3. **Next time:** I would start with a `structureSearch` tree or a files-only `localSearch` to get the layout. I would pass the array parameters correctly. I would add a `localSearch` for `GetExternalModuleIndicatorOptions` to see its callers. I would also fetch a wider range so every cited line is one I actually saw.

4. **Confidence:** High for the core logic, because I read those lines directly. Medium-high for completeness, because I skipped the call sites.

## L20

1. **Helped:** The `localSearch` for `ResolveJsonModule` (excluding test and generated files, `contextLines: 2`) was the key call. It listed every non-test Go caller with context in one pass. The `localFetch` of `compileroptions.go` lines 268-285 gave the exact default logic. The first `localSearch` with `resultView: files` also showed the checkout contained the Go compiler under `tsc/internal`.

2. **Did not help:** My first two calls failed on schema validation. I left out the `queries` wrapper and passed strings where arrays were expected. My second attempt also omitted the required `goal` and `reasoning`. The `structureSearch` for Go files returned only 20 of 5118, so it was noise. The `localFetch` output showed paths relative to a base directory, which meant I had to reconstruct full paths myself.

3. **Next time:** I would check the schemas before the first call and send well-formed queries at once. I'd skip the file-listing query. I would also open the callers' bodies, `declscompiler.go`, the diagnostics file and the test files, which I never read. I would use `lspSearch` references on `GetResolveJsonModule` to confirm the caller list, since text search can miss aliased uses.

4. **Confidence:** Medium-high. The default logic and caller list are directly observed. The gap is that I did not read the caller bodies or the tests, so my impact claims for them are inferred from names and snippets.
