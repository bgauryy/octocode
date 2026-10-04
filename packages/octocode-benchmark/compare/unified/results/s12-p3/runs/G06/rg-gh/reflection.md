**1. Helped**
- The second Bash call (`gh api -X GET repos/encode/httpx/contents/httpx/_client.py -f ref=b5addb64f0 ... | base64 -d | grep -n "proxy|_mounts|..."`) was the key step. One call gave every relevant line number at the pinned commit.
- The third call (`sed -n` on specific line ranges) confirmed the exact code for `_get_proxy_map`, the mounts construction and `_transport_for_url`.
- The fourth call (`sed -n '/def get_environment_proxies/,...'`) showed the `NO_PROXY` handling.

**2. Did not help**
- The first Bash call tried to save the file to `/tmp/c.py`. The sandbox blocked it, so that call was wasted.
- Every Bash call printed `/dev/null: Operation not permitted`. This was harmless noise, but it made me check each time that the output was still valid.
- I never read `URLPattern.matches` or `__lt__` in `_utils.py`. My claim about pattern ordering is therefore inferred, and I flagged it as such.

**3. Next time**
- Pipe the API output straight into grep or sed from the start, with no temp files.
- Add one more call to read `URLPattern` in `_utils.py`.

**4. Confidence**
- High for the overall flow (proxy map → mounts → first matching pattern, `None` meaning direct). I saw all of that code directly.
- Medium-low for how patterns are matched and ranked, because I did not read that code.