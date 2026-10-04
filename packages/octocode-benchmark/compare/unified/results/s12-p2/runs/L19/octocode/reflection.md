**1. Helped:**
- `localSearch` for `ModuleDetection` across `tsc/` with `*.go` and the test exclusion. It found the option enum, the default resolver, and the `parseoptions.go` consumer in one call.
- Parallel `localFetch` calls: the full `parseoptions.go` and the range 240-253 of `compileroptions.go`. They gave the deciding logic with exact line numbers.
- The follow-up `localSearch` for `SetExternalModuleIndicator|ExternalModuleIndicatorOptions`. It connected the parse options to `fileloader.go:422` and `parser.go:486`.

**2. Did not help:**
- My first `localSearch` used a guessed path (`<repo>/internal`) and returned `pathNotFound`. I should have run `structureSearch` first, as the error hint suggested. The Go code is under `tsc/internal`.
- I could not confirm the checkout is at commit 4f5ddae224. I relied on the prompt's statement and have no tool for checking it.
- I did not trace downstream consumers of the indicator (checker, emit), or `contentmapper/transform.go:62,89`.

**3. Next time:** Run `structureSearch` on the repo root before any path-based search. Then run the literal `localSearch`, followed by one batched fetch. I would also use `lspSearch` on `ExternalModuleIndicator` to see its consumers if the question asked about downstream behavior.

**4. Confidence:** High on the decision logic and line numbers, since I read them directly. Medium on completeness: the downstream consumers and the `contentmapper` special case are unchecked, and I said so in the answer.