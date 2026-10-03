1. **Helped:** One Bash call did nearly all the work. It chained `gh api -X GET repos/nodejs/undici/pulls/5881` (title, body, merge SHA) with `.../pulls/5881/files`, which returned the full patch for all four files. That gave the cause, the mechanism and the tests with no searching. The `--jq` filters kept the output compact.

2. **Did not help:** The shell printed `/bin/bash: /dev/null: Operation not permitted` three times. It was harmless but noisy. I made only that one call, so nothing was repeated. I did not read the merged source, so the diff stands in for the code at the pinned commit. I also cited no `path:line` values, only file and symbol names from the patch. I could not run the tests either.

3. **Next time:** I would add a second call to `gh api repos/nodejs/undici/contents/lib/dispatcher/pool-base.js?ref=da911f08...`. That would let me cite real line numbers at the merge commit. I would also check the linked issue or review comments for the original bug report.

4. **Confidence:** Medium-high. The diff and PR body are primary evidence, and the mechanism is clear from them. Remaining risk is the unverified line numbers and not confirming the merged state.