# rg-gh: reflection after 2 questions

# REFLECT.md

## What helped

- **G03:** One parallel Bash call did the whole job: `gh pr view 5881 -R nodejs/undici --json title,body,state,mergeCommit,files,url` chained with `gh pr diff 5881 -R nodejs/undici`. The diff showed the mechanism directly (`kRetireClient`, `kRetiring`, `closeClients`, the `kDestroy` change, the `connectionError` handlers). The removed comment in `pool.js` and the new test file confirmed the intent. The JSON `mergeCommit` field supplied the SHA.
- **L14:** The first call combined `git log -1 --format=%H` with `rg -n "function debounce"`. It confirmed the pinned SHA and located `lodash.js:10403` in one step. A second call printing lines 10403-10530 was enough to answer.

## What did not help

- **G03:**
  - The PR body was thin ("Bug Fixes: N/A"), so the diff had to carry the explanation.
  - I never opened the source at the merge commit, so I had no post-merge line numbers.
  - I did not read the `clientTtl` eviction path. That claim rests on the diff and tests only.
- **L14:**
  - The `ls debounce.js` in the first call was a guess and failed. It was harmless but unnecessary.
  - The `cat -n | awk` numbering pipeline stripped indentation, and its line numbers depended on my `sed` offset being right. The offset matched the `rg` hits at 10403 and 10499, which supports the citations but does not fully verify them.
  - I did not read the JSDoc above `debounce` (roughly lines 10370-10402), which would have given the documented `maxWait` semantics.
  - I did not diff `dist/lodash.js` against `lodash.js`. I only saw `function debounce` at 10403 in both, so the claim that they match is unverified.

## Patterns

- Both questions were answered from a few calls. Combining a location or metadata step with a read step in one Bash call worked well.
- Both answers stopped short of one verification step: the code at the pinned or merge commit in G03, and the JSDoc and dist diff in L14. Each time I disclosed the gap rather than closing it.
- Line-number citations are only as reliable as the command that produced them. Post-processed output (L14) and missing source reads (G03) both weakened them.

## Suggested changes

1. **Prefer native line numbers.** Add guidance to use `rg -n -A<N>` or `sed -n 'a,bp'` on the file, and not to renumber with `cat -n | awk`. This keeps indentation and makes the citations trustworthy.
2. **Add a PR-research checklist.** After `gh pr view` plus `gh pr diff`, also fetch review comments and linked issues (`gh pr view --comments`). Then read the touched files at the merge SHA via `gh api` or a local checkout, so answers can cite post-merge `path:line`.
3. **Require a check before "same as" claims.** Diff the two files (for example `diff` or `cmp`) before saying two copies match, such as `dist/` versus source. If unchecked, say so.
4. **Read the doc comment.** When explaining a function's behavior, read the JSDoc or docstring above it along with the body.
5. **Avoid guessing filenames.** Use `rg -n` or `rg --files -g` to find a file first, not `ls` on a guessed path.

---

# Per-question reflections

## G03

1. **Helped:** One parallel Bash call, `gh pr view 5881 -R nodejs/undici --json title,body,state,mergeCommit,files,url` chained with `gh pr diff 5881 -R nodejs/undici`, returned everything I needed. The diff shows the exact mechanism (`kRetireClient`, `kRetiring`, `closeClients`, the `kDestroy` change, the `connectionError` handlers). The removed comment in `pool.js` and the new test file confirmed the intent. The JSON `mergeCommit` field gave me the SHA.

2. **Did not help:** Nothing was wasted, since I made only one call. The PR body is thin, with "Bug Fixes: N/A", so the diff had to carry the explanation. I did not open the source at the merge commit, so I could not give post-merge line numbers. I also didn't check the `clientTtl` eviction path beyond what the diff and tests show. The answer says both of those limits.

3. **Next time:** I would fetch the PR review comments and linked issues (`gh pr view --comments`) to see maintainer discussion. I would also fetch `pool-base.js` at `da911f0` with `gh api` or a local checkout, to cite line numbers and confirm how `kRemoveClient` is called on the TTL path.

4. **Confidence:** High on the bug and mechanism, because they come directly from the diff and the PR text. Medium-high on the `clientTtl` detail, since I inferred it from the tests and did not read the eviction code.

## L14

**1. Helped:** My first call combined `git log -1 --format=%H` with `rg -n "function debounce"`. It confirmed the checkout was at the pinned SHA and found `lodash.js:10403` in one step. The second call printed the whole function (lines 10403-10530), which was enough to answer the question.

**2. Did not help:**
- The `ls debounce.js` in the first call was a guess and failed. It was harmless but unnecessary.
- The `cat -n | awk` line-numbering pipeline in the second call was clumsy. It stripped the indentation, and the line numbers depended on my `sed` offset being right. A plain `rg -n` or `sed` on the range would have kept the original formatting and been easier to trust.
- I did not read the JSDoc above `debounce`. It is at roughly lines 10370-10402 and would have given the documented semantics for `maxWait`. I said so in the answer.
- I did not check whether `dist/lodash.js` differs from `lodash.js`. I saw both had `function debounce` at 10403. But I never diffed them, so my statement that they have the same code at the same lines is unverified. The match was only on the two function-signature lines.

**3. Next time:** I would use one `rg -n -A130 "function debounce" lodash.js` call, or `sed -n` on the range, to read the code with correct numbers. I would also read the JSDoc and diff the two files before making any claim about `dist/`.

**4. Confidence:** High for the logic, because I read the source directly at the pinned commit and quoted it with line numbers. Medium-high for the line citations, because they came from my awk offset. That offset matched the `rg` hits at 10403 and 10499, which supports them but is not a full check. The `dist/lodash.js` claim is the weakest part.
