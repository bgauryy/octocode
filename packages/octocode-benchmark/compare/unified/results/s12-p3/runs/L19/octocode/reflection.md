**1. Helped:** The `localSearch` for `ModuleDetection` over `tsc/` (with `*.go` include and `_test.go` exclude) found every relevant file in one call. `ast/parseoptions.go` and `core/compileroptions.go` stood out immediately. The two `localFetch` calls then gave the deciding lines: the whole of `parseoptions.go`, and lines 240-253 of `compileroptions.go`. Those two fetches covered nearly the whole answer.

**2. Did not help:**
- My first `localSearch` failed with `pathNotFound` because I guessed `internal/` at the repo root. That was an avoidable error. The `structureSearch` that followed showed the Go code lives under `tsc/`.
- The parallel `localSearch` on `packages/` returned nothing. It was a speculative call that cost little but added nothing.
- I never read `GetImpliedNodeFormatForEmitWorker`, and I never found where `SetExternalModuleIndicator` is called. No tool failed me there. I stopped early.

**3. Next time:** Run `structureSearch` first instead of guessing the layout. Then use `lspSearch` or a `localSearch` on `SetExternalModuleIndicator` to confirm the call path from the parser. I would also read the implied-format function so that claim doesn't rest on a code comment.

**4. Confidence:** High on the decision logic, since it comes from code I read directly with line numbers. Medium on completeness, because the call site and the implied-format function are unverified, and I said so in the answer.