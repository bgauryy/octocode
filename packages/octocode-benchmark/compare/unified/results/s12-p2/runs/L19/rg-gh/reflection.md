1. **Helped:** The `grep -rn "ModuleDetection" --include=*.go .` call (after `rg` failed) listed every relevant symbol at once. It led straight to `core/compileroptions.go:243` and `ast/parseoptions.go:24`. Two `sed` reads of `parseoptions.go` then gave the full decision logic with line numbers.

2. **Did not help:**
   - The first call used `rg`, which isn't installed, so that search was wasted.
   - Every Bash call printed `/dev/null: Operation not permitted` noise.
   - `git log -1` failed, so I could not confirm the checkout was at 4f5ddae224. My answer says the SHA is unverified.
   - I never read the `GetImpliedNodeFormatForEmitWorker` code, so how a `.js` file's `type: module` scope is determined is untraced.

3. **Next time:** Start with `grep`, since `rg` is missing. Check the SHA by reading `.git/HEAD` with `cat` instead of `git`. Spend one extra grep on `GetImpliedNodeFormatForEmitWorker` to close the `.js` gap.

4. **Confidence:** High for the main logic. I read it directly, and line numbers came from the `sed` and `grep` output. Medium on the pinned-commit claim, since I could not verify the SHA and I did not run the code.