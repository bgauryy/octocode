**1. Helped:** `ghGetFileContent` on `prometheus/client_golang` with `branch: "v1.20.0"` and `matchString: ["DefBuckets"]` did most of the work. It returned the `DefBuckets` declaration (line 265), the `Buckets` doc comment (391-396) and the fallback code (563-565) in one call, at the pinned ref. The first call, `ghSearchCode` for `DefBuckets` in `histogram.go`, located the file and symbol quickly.

**2. Did not help:** `ghSearchCode` only searches the default branch. It returned commit 0ecb5c2, not v1.20.0, and its line numbers (271, 574) differ from the tag's, so I had to re-read at the tag. `matchString` omitted lines 272-386 and 400-557, so I never saw the native-histogram options in full. I also never confirmed that the tag resolves to 73b811c. I took that from the tool's returned `commitSha`.

**3. Next time:** I'd go straight to `ghGetFileContent` with the tag. I'd also run `ghSearchRepo` to confirm the repo is the official one, since I relied on prior knowledge for that.

**4. Confidence:** High for the bucket values and the two conditions where defaults don't apply, since I read both in the tagged source. Medium-high for the repo identity, because no tool verified it.