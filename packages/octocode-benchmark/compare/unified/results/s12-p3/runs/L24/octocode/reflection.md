**1. Helped:**
- The first `localSearch`, with a regex for `go func` and `go x.y(` and `exclude: ["*_test.go"]`, found all seven `go` statements in two files. It used `contextLines: 0`, so the output was minimal.
- The batched `localFetch` with line ranges for both files showed each goroutine body in one call.

**2. Did not help:**
- The last two `localSearch` calls, listing function-start lines, partly overlapped. The `Manager` method listing was mostly redundant.
- The `localFetch` ranges had large omitted gaps. I therefore never saw the start of `restartLoops`, `sync` or `ApplyConfig` in fetched bytes, and attributed goroutines to them by line number.
- My answer said `restartLoops` and `sync` hold "one or two each". Actually `restartLoops` has one and `sync` has two.
- I never read `Manager.reloader` or `scrapeLoop.run`.
- I did not search for non-`go` launch patterns such as `errgroup` or `.Go(`. I did flag the regex limitation in the answer.

**3. Next time:**
- Use `astSearch` for the `go` statements, since it should give the enclosing function directly.
- Fetch `reloader` and the loop `run` function.
- Run one search for `.Go(`.

**4. Confidence:** medium-high. The seven `go` statements and their bodies are directly observed, and the line-number attribution to functions is consistent. Completeness for other launch patterns and for goroutines inside `run` is unverified.