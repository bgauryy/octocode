**1. Helped:**
- The first `localSearch` was the most useful call. It had an `include` filter for `config/config.go` and `go.mod`, and in one call it returned the field type at `config.go:851` and the root pin `go.mod:69` (v0.71.0). It also listed the sub-module pins.
- `ghGetFileContent` on `prometheus/common` with `branch: v0.71.0` and `matchString: ValidationScheme` returned the enum, `Set`, and the validators with line numbers.
- The follow-up `ranges` call filled in the omitted lines.

**2. Did not help:**
- My second `localSearch` looked for `^replace` in `config.go` instead of `go.mod`. That was a wasted call and a meaningless check. My answer said no `replace` was found, which was misleading even though I added that I hadn't searched `go.mod`.
- I never read `isValidLegacyRune`, so the legacy metric-name character set came from memory, not evidence. I did flag this in the answer.
- I cited `config.go:1005` ("scrape config replaces unset with the global value") from grep match lines only, without reading the lines around it.
- The omitted-lines output from `ghGetFileContent` forced an extra call.

**3. Next time:**
- Search `go.mod` directly for `replace`.
- Fetch `isValidLegacyRune` in the same ranges call as the other lines.
- Read `config.go:995-1010` before making claims about it.

**4. Confidence:**
- High for the module and version (v0.71.0) and the accepted values (`""`, `legacy`, `utf8`).
- Medium for the legacy metric-name character set and the unset-to-global behaviour, which I didn't confirm from source.