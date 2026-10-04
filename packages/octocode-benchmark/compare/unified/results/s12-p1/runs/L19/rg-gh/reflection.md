1. **Helped:** The first `grep -rn "ModuleDetection" --include=*.go` (after `rg` failed) found every relevant site in one pass. It pointed to `core/compileroptions.go:243` and `ast/parseoptions.go:19`. The second call printed those two regions plus a grep for the indicator functions, which gave me the whole decision chain and the `parser/parser.go:486` call site.

2. **Did not help:**
   - `rg` isn't installed, so my first call was wasted.
   - Every shell call printed `/dev/null: Operation not permitted`, which is noise.
   - I never ran `git rev-parse HEAD`, so I didn't confirm the checkout was at 4f5ddae.
   - I never opened `isAnExternalModuleIndicatorNode` or `isFileModuleFromUsingJSXTag`.

3. **Next time:** Use `grep` from the start. Confirm HEAD. Print the full function bodies with `sed -n`, with line numbers (`cat -n`).

4. **Confidence:** High on the overall logic, because I read the code directly. Two defects in my answer:
   - I wrote `core/compilerOptions.go`, but the real filename is `compileroptions.go` (lowercase).
   - I cited `parseoptions.go:56-86`, but my output cut off before I saw where that range ends. The end line is a guess.