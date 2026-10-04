1. **Helped:** The second Bash call fetched `sessions.py` at the pinned ref with `gh api -X GET "repos/psf/requests/contents/src/requests/sessions.py?ref=611c6162cb" --jq .content | base64 -d | cat -n | sed -n '90,330p'`. One call returned `resolve_redirects` with numbered lines, and `cat -n` gave me exact line numbers to cite. The third call read the rest (`rebuild_*`, `Session.send`) and grepped for `allow_redirects` and `history`.

2. **Did not help:**
   - The first call listed an unrelated local `python` checkout, which was wasted.
   - It also tried to write to `/tmp`, which failed with "Operation not permitted".
   - Every Bash call printed `/dev/null: Operation not permitted`. This was noise, though the output was still correct.
   - I could not follow into other files.

3. **Next time:** I'd skip the local listing and go straight to piping `gh api` output. I'd also fetch `models.py` and `utils.py` in the same call, so `is_redirect`, `rewind_body` and `resolve_proxies` are verified rather than left as caveats. I would also read past `sessions.py:821` to see what `send` does with the `yield_requests` result.

4. **Confidence:** High for the behavior in `sessions.py`, since every line number came from code I read at the pinned commit. Medium for anything depending on the helper functions in other files, which I flagged as unverified.