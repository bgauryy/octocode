1. **Helped:** Fetching `sessions.py` once at the pinned ref with `gh api "repos/psf/requests/contents/src/requests/sessions.py?ref=611c6162cb" --jq .content | base64 -d` gave me the whole file. A single `grep -n` for `def resolve_redirects|def rebuild_*|get_redirect_target|should_strip_auth` located every function. Then one `sed -n 125,400p` and one `sed -n 798,830p` showed the redirect loop and the `Session.send` call site. That was about four useful calls in total.

2. **Did not help:**
   - My first `gh api` call failed, because zsh globbed the unquoted `?` in the URL. I had also run `ls` on the local `repos/python` directory, and it held no requests checkout, so that was wasted.
   - My `sed | cat -n | awk` line-numbering pipeline stripped the indentation and mangled the output, but the line numbers stayed usable. I also ran a grep for `allow_redirects` whose output I only partly used.

3. **Next time:** I'd quote the URL on the first call. I'd use `sed -n` or `cat -n` to get line numbers, not awk. I'd skip the local `ls` and go straight to `gh`.

4. **Confidence:** High on the mechanics and line numbers, since they come from source I read at the pinned commit. I did not run anything. I also did not open `models.py` to check `is_redirect`, and I did not check the `DEFAULT_REDIRECT_LIMIT` value. I stated both gaps in the answer.