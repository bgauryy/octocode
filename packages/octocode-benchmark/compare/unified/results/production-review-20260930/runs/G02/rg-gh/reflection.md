**1. Helped:**
- `gh pr view 13824 --json ...files,mergeCommit` gave the file list and merge SHA in one call.
- `gh pr diff 13824` gave the whole change. I saved it to `/tmp/p.diff` and used `awk` to pull out the Python-side and `validators/counter.rs` hunks.
- The new `tests/types/test_counter.py` in that diff showed the intended behaviour directly.
- The last call fetched the parent commit's `_generate_schema.py` via `gh api contents?ref=<parent>`. It showed the "before" `_mapping_schema`.

**2. Did not help:**
- My first `gh api` call for the old file failed with a zsh glob error (`no matches found`) because the `?` wasn't quoted. I also guessed `~1` as the ref.
- The first diff output was cut off by `head -c`, so I needed a second pass.
- I never found an old test or run that showed the "before" error type. I didn't build or run anything.

**3. Next time:**
- Quote URLs with `?`.
- Save the diff to a file at the start.
- Fetch the parent SHA first, then the old files.
- Check the old `tests/types/test_counter.py` and grep for `dict_type` to confirm the old error.

**4. Confidence:** medium-high.
- The new behaviour is read straight from the diff, and the tests back it up.
- The "before" `dict_type` error and the claim that length constraints didn't apply are inferences from the old schema code and the PR text. I didn't run them.