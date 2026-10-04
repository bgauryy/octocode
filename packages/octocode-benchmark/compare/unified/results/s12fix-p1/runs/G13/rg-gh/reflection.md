1. **Helped:** `gh api -X GET repos/prometheus/node_exporter/contents/node_exporter.go?ref=v1.8.2 -q .content | base64 -d` was the key step. It fetched the file pinned to the tag, and the line numbers I cited came straight from it. Piping into `grep -n` worked well once I dropped `rg`.

2. **Did not help:**
   - The first call failed because `rg` is not installed, despite the instructions mentioning it. That cost one call.
   - Every Bash call printed `/dev/null: Operation not permitted`. It was harmless noise, but it could mask real errors.
   - My first grep missed the listen address. It is passed as a positional argument (`":9100"`) to `kingpinflag.AddFlags`, not in a `.Default()` chain. The second call was needed to find it.
   - I never read the exporter-toolkit source, so I could not confirm the flag name `--web.listen-address` from evidence. I flagged that in the answer.

3. **Next time:** I would check which tools exist before using them. I would grep for `9100` and `web.` in the first call. If I wanted to confirm the flag name, I would also fetch the exporter-toolkit `kingpinflag` file.

4. **Confidence:** High for the repo, metrics path (`/metrics`) and max requests (`40`), and for `:9100` as the default listen address. Each of these was read from the v1.8.2 file with line numbers. Medium-high for the flag name only, since that part came from memory.