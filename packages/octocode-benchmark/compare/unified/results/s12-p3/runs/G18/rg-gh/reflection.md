1. **Helped:** `gh api -X GET repos/nodejs/undici/contents/...?ref=v6.21.0 --jq .content | base64 -d | grep -n` on `package.json` and `lib/dispatcher/client.js` gave the engines field and the default timeout lines (229, 238, 239) directly. Cross-checking `docs/docs/api/Client.md` confirmed the values. The `git/ref/tags/v6.21.0` call gave the commit SHA, and a follow-up showed it was a commit object, not an annotated tag.

2. **Did not help:**
   - My first calls used `rg`, which isn't installed, so I wasted two calls.
   - The `/dev/null: Operation not permitted` errors came from the sandbox and added noise to every call.
   - I put `curl` to the npm registry in the first call and again in the third. It produced no output, so I never saw the registry's `gitHead`. The sandbox blocks direct networking, and I should have known that. I disclosed this in the answer.

3. **Next time:** Use `grep` from the start. Skip `curl` entirely. Try `gh api` for the published commit, such as a release or a "Publish 6.21.0" commit, before relying on the tag.

4. **Confidence:** High on the Node range and the timeout defaults, since I read them from source and docs at the tag. Medium-high on the SHA, because it comes from the tag, not from the published tarball.