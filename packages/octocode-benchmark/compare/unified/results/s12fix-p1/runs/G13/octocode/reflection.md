**1. Helped:** The second call, `ghGetFileContent` on `node_exporter.go` with `branch: v1.8.2` and `matchString` for `telemetry-path`, `max-requests` and `listen-address`, answered most of the question. It returned the pinned-commit lines for the `/metrics` path and the `40` request limit. The third call's first query (`matchString` for `toolkitFlags`) found `kingpinflag.AddFlags(..., ":9100")` at line 179.

**2. Did not help:**
- The first `ghSearchCode` call was wasted. The `web.listen-address` query only hit test files, and the `web.max-requests` query came back empty. Indexed search covers the default branch only, and it can't pin a tag.
- The `exporter-toolkit` read in the third call was at the default branch, not the version v1.8.2 pins. It only confirmed the flag's name and behaviour, and I said so in my answer.
- I never confirmed the tag-to-commit mapping separately. I relied on the SHA the tool returned for `branch: v1.8.2`.

**3. Next time:** Skip the code search and go straight to `ghGetFileContent` with the tag and `matchString`. If the toolkit default mattered, I would read `go.mod` at the tag and fetch the toolkit at that pinned version.

**4. Confidence:** High for all three values, since each is cited to the pinned v1.8.2 source. The only thing I didn't verify is the toolkit's behaviour at the exact pinned version, and it doesn't affect the `:9100` default.