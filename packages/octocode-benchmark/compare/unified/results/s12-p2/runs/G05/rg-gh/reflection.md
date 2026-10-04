1. **Helped:** The second Bash call fetched `sessions.py` at the pinned commit with `gh api -X GET "repos/psf/requests/contents/src/requests/sessions.py?ref=611c6162cb" --jq .content | base64 -d | grep -n "" | sed -n '95,400p'`. Numbering every line with `grep -n ""` and then slicing with `sed` showed `resolve_redirects`, `rebuild_auth`, `rebuild_proxies` and `rebuild_method` in one pass, with accurate line numbers. The third call, a grep for `allow_redirects|resolve_redirects|max_redirects|history`, located the `Session.send` wiring.

2. **Did not help:**
   - Every shell call printed `/dev/null: Operation not permitted`. This was sandbox noise rather than a failure, but it cluttered the output.
   - My first call tried to write `/tmp/sessions.py`, which the sandbox blocked. It also guessed at local checkouts that turned out to be irrelevant.
   - I never read `Session.send` past line 821, nor `Response.is_redirect`, so two details stayed unverified. I disclosed both in the answer.

3. **Next time:** I would skip the local-directory guess and stream from `gh api` straight into `sed`. I would also fetch lines 760-830 of `sessions.py` and grep `models.py` for `is_redirect` in the same call, so nothing is left unverified.

4. **Confidence:** High for the redirect loop and the per-hop changes, because I read that code directly at the pinned commit. Medium for the `allow_redirects=False` branch and for which status codes count as redirects, since I only inferred those.