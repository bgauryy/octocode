**1. Helped:**
- The first `localSearch` on `config/config.go` and `go.mod` was the fastest step. It gave the field declaration (line 851), the type `model.ValidationScheme`, and the `go.mod:69` pin to v0.71.0 in one call.
- `ghGetFileContent` with `branch: "v0.71.0"` read `model/metric.go` at the exact pinned tag. It gave the enum, the `Set` values, and the doc comments.

**2. Did not help:**
- The `ghSearchCode` call hit the default branch (commit `ca4f6e15`), not v0.71.0. It was only a lead and could have misled me on version.
- The second `localSearch` on `go.mod` repeated the first call's `go.mod:69` hit. It only added the `assets` line.
- I never checked `go.mod` for a `replace` directive. I said so in the answer.
- I never read `MetricNameRE` or `LabelNameRE`, so the exact legacy character set is unverified.

**3. Next time:**
- Skip the default-branch code search and go straight to `ghGetFileContent` at the tag.
- Add one more read for the regex definitions.
- Run a `localSearch` for `replace` in `go.mod`.

**4. Confidence:** High on module, version and the three values, since I read them at the pinned tag. Medium-high on the claim that the pin isn't overridden, because I didn't scan the whole `go.mod` for `replace`. The legacy rule is cited from a comment only.