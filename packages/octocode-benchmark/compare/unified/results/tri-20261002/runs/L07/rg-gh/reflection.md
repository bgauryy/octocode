1. **Helped:** The first Bash call (`cat redirect.ts` plus one `rg` for `isRedirectError|getURLFromRedirectError|getRedirectStatusCodeFromError` under `server/`) found the throw site and every consumer in one step. The second call (`sed` ranges around the `rg` hits) showed the response-building code in the page, route-handler, action and meta-tag paths.

2. **Did not help:** Every Bash call printed `/bin/bash: /dev/null: Operation not permitted`. That was harmless but noisy. I used `cat` and `sed` without `-n`, so most of the output had no line numbers. I never checked the commit with `git rev-parse` or similar.

3. **Next time:** Use `rg -n` or `cat -n` so every cited line is one I saw. Also read `createRedirectRenderResult` and look for other redirect handlers (static generation, prerender).

4. **Confidence:** High on the mechanism: throw an error with a digest, parse it, set status and `Location`. Medium on the exact line numbers.
   - Seen: `rg` hits (4386, 4388, 4392, 9833, 9841, 9844, 424, 447, 1324, 56, 58, 61).
   - Estimated, so treat as approximate: `redirect.ts` 9-17, 38-45 and 62-67; `redirect-error.ts` 15-40; most range end points. I should have flagged these in the answer.