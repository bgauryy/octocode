1. **Helped:** Fetching `sessions.py` with `gh api "repos/psf/requests/contents/src/requests/sessions.py?ref=611c6162cb"` piped through `base64 -d` into `/tmp/sessions.py`. That pinned the exact commit and gave me one file. The `grep -n "def resolve_redirects|def rebuild_..."` and the `sed -n 95,400p` read covered the whole redirect path in about two calls. A final `grep` for `resolve_redirects|allow_redirects` connected `Session.send` to the loop.

2. **Did not help:**
   - My first call errored. I didn't quote the URL, so zsh tried to glob the `?`. That cost one round trip.
   - The same call also listed the unrelated local `repos/python` directory. That was noise, and I never used the local checkouts.
   - The two closing greps could have been one call.

3. **Next time:** Quote the `gh api` URL from the start. Combine the `send` wiring grep into the first pass. Check `models.py` for `is_redirect`, which I skipped and flagged as a gap.

4. **Confidence:** High for the mechanics and line numbers. They come from the pinned-commit file I read directly, and I cited only lines I saw. Medium-high overall, because I did not run the code and did not verify which status codes count as redirects.