**Helped:**
- `localSearch` for `ModuleDetection` across `tsc/`, with `include *.go` and test files excluded, gave every relevant site in one call. That made `parseoptions.go` and `compileroptions.go` obvious.
- Two `localFetch` calls were enough. One read all of `parseoptions.go`, and one read the line range `compileroptions.go:240-253` for the default resolution. Both returned numbered lines I could cite directly.

**Did not help:**
- My first `localSearch` assumed an `internal/` directory at the repo root. It failed with `pathNotFound`. I should have run `structureSearch` first. The Go code is under `tsc/`, not at the root.
- I did not trace callers of `GetExternalModuleIndicatorOptions` or `SetExternalModuleIndicator`. I said so in the answer.
- I only saw one grep line for `ls/autoimport/fix.go:915`. Calling it "auto-import behaviour" was inferred from the file path, not from reading the surrounding code.

**Next time:** run `structureSearch` before guessing a path. Add one `lspSearch` references call on `GetExternalModuleIndicatorOptions` to confirm the parser wiring. Read around `fix.go:915` before describing it.

**Confidence:** high for the decision logic and defaults, since I read the exact lines. Medium for the caller wiring and the auto-import claim, since I did not verify them.