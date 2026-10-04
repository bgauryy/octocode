1. **Helped:** The second Bash call fetched `node_exporter.go?ref=v1.8.2` through `gh api`, decoded it with `base64 -d` and ran `grep -n -E`. It located the flag definitions at the pinned tag. The `sed -n 158,200p` call then showed the defaults `/metrics`, `40` and `":9100"` directly. The README grep and the `go.mod` grep confirmed port 9100 and the exporter-toolkit v0.11.0 pin.

2. **Did not help:**
   - The first call used `rg`, which isn't installed, so it was wasted despite the prompt suggesting `rg`.
   - Every Bash call printed `/dev/null: Operation not permitted`. It was harmless but noisy.
   - My `~` line numbers were unnecessary. The `sed` range had no line numbers, and I never re-ran it with `nl` or `grep -n`.
   - I never opened the exporter-toolkit source, so the `--web.listen-address` flag name rests on memory, not evidence.

3. **Next time:** Use `grep` first. Fetch the file once into a variable, then run `grep -n` for exact line numbers on the defaults. Also fetch exporter-toolkit v0.11.0's `kingpinflag/flag.go` to confirm the flag name.

4. **Confidence:** High for the repo and the defaults `/metrics`, `40` and `:9100`, since I read them at the v1.8.2 tag. Medium for the exact line numbers and the flag name.