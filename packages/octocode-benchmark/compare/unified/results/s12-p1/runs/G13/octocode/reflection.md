**1. Helped:** `ghGetFileContent` on `node_exporter.go` with `branch: v1.8.2` and a `matchString` array was the fastest step. It returned the `web.telemetry-path` (`/metrics`) and `web.max-requests` (`40`) defaults with line numbers and the pinned commit SHA. The follow-up `ghGetFileContent` with `matchString: ["AddFlags", "toolkitFlags"]` surfaced `":9100"` at line 179.

**2. Did not help:**
- The first `ghSearchCode` returned empty. It searches the default-branch index, not the v1.8.2 tag, so it couldn't have proven anything about that release anyway.
- The `ghGetFileContent` call on exporter-toolkit's `web/kingpin_flag.go` was a guessed path and returned a 404. I should have used `ghStructure` instead. It also read the default branch, not the version node_exporter v1.8.2 pins.
- Because that call failed, I never confirmed the flag name `--web.listen-address`. I stated it from knowledge of the toolkit, which my answer did say.
- I never ran `ghSearchRepo`, so "official" rests on the `prometheus` org name, not a check.

**3. Next time:** Skip the code search and go straight to a tag-pinned `ghGetFileContent`. Use `ghStructure` before reading any path I haven't seen. Check the toolkit at the version in node_exporter's `go.mod` to confirm the flag name.

**4. Confidence:** High on `/metrics` and `40`, since I read them at the pinned commit. High on `:9100`, which I read in the source. Medium-high that the flag name is `--web.listen-address`, since that part is unverified.