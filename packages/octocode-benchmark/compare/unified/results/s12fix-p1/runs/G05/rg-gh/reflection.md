**1. Helped:** The working call was `gh api -X GET "repos/psf/requests/contents/src/requests/sessions.py?ref=611c6162cb" --jq .content | base64 -d | awk 'NR>=95&&NR<=340...'`. It pinned the commit and gave line-numbered output I could cite directly. The follow-up call covered lines 340–420 and picked up the `allow_redirects` and `history` lines in `Session.send`. Together they covered the whole redirect path.

**2. Did not help:**
- My first call was wasted. I ran `ls` on the local `python` checkout, which was unrelated to requests, and tried to write the file to `/tmp`, which was blocked.
- Every Bash call printed `/dev/null: Operation not permitted`. It was harmless but noisy.
- I never read `Response.is_redirect` (models.py) or `rewind_body` (utils.py). I flagged both as unverified.
- I did not check `resolve_proxies` or `extract_cookies_to_jar`.

**3. Next time:** Start with the piped `gh api` fetch, skip the local directory listing, and avoid writing to disk. In the same call, also fetch the `is_redirect` property and `rewind_body`, using `rg` on the piped stream or a second `gh api` call in parallel.

**4. Confidence:** High for the claims about `sessions.py` (line-cited, read at the pinned commit). Medium for the `allow_redirects=False` branch at line 821, because I only saw the call and not what happens to its result. I flagged the unverified parts in the answer.