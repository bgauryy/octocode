**1. Helped:** My first Bash call, `gh api -X GET repos/prometheus/node_exporter/contents/node_exporter.go?ref=v1.8.2 | base64 -d | grep`, pinned the tag and found the flag definitions. The second call, `sed -n 155,200p` on the same file, showed the defaults `/metrics`, `40` and `:9100` in context.

**2. Did not help:**
- Every call printed `/dev/null: Operation not permitted`. It was harmless but noisy.
- My first grep matched too broadly and did not show the `Default(...)` values, so I needed the second call.
- The `exporter-toolkit` lookup at `web/kingpin_flag.go` returned a 404. I guessed the path and never retried, so I did not confirm that `AddFlags` registers `--web.listen-address`. I stated it from prior knowledge, and the answer does say that.
- I called the line numbers approximate, though the numbered grep output let me derive them exactly. The `:9100` call is probably line 179, not 180.

**3. Next time:** I would run one `sed -n` with `cat -n` so line numbers are exact. I would find the toolkit file with `gh api` on the repo tree or code search, and pin the toolkit version from `go.mod` at v1.8.2.

**4. Confidence:** High on the repo and on the `/metrics` and `40` defaults, which I saw directly. Medium-high on `:9100`, since the toolkit flag name is unverified. Low-to-medium on the exact line numbers.