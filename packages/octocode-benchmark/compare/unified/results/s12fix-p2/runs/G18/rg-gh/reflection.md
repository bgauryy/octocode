1. **Helped:** `gh api -X GET repos/nodejs/undici/git/ref/tags/v6.21.0` gave the tag commit SHA in one call. Reading `package.json` and `lib/dispatcher/client.js` through `contents/...?ref=v6.21.0`, piped into `base64 -d | grep -n`, gave the `engines` field and the default-timeout assignments with line numbers. Fetching `docs/docs/api/Client.md` the same way cross-checked those defaults.

2. **Did not help:**
   - The first call combined `curl` against the npm registry with the tag lookup. `curl` was blocked, so I never saw the published `gitHead`, which left the publish-commit part unverified.
   - The second call used `rg`, which isn't installed, so I had to redo it with `grep`.
   - Every shell call printed `/dev/null: Operation not permitted`. That was harmless noise but made me check each result more carefully.
   - A lightweight tag could have pointed at a tag object rather than a commit. This one was a commit, so it didn't matter.

3. **Next time:** use `grep` from the start. Try `gh api` for npm metadata, if a gateway route exists, to get `gitHead`. Combine the fetches into one call.

4. **Confidence:** high on Node `>=18.17` and the timeout defaults, because I read them directly in the source at the tag. Medium on the publish commit, because it rests on the tag and not on the registry's `gitHead`.