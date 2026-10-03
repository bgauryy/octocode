1. **Helped:** My first Bash call was the most useful. It combined `cat client/components/redirect.ts` with one `rg` for `isRedirectError|getURLFromRedirectError|getRedirectStatusCodeFromError` over `server/`. That showed the throw side and every catch site in a single pass. The second call printed line ranges around each hit, which gave me the exact code to cite. The third call, an `awk` that finds the enclosing function, resolved the names `renderToStream` and `prerenderToStream`.

2. **Did not help:**
   - The second call printed a lot of surrounding code I didn't need. I also guessed at the range for `redirect.ts:~70-93` and never printed it exactly, which is why that citation is approximate.
   - The `awk` call was a workaround for not having a symbol-aware tool.
   - I never opened `createRedirectRenderResult`, so the fetch-action header path is unverified.
   - I did not confirm the commit SHA with `git`. I relied on the checkout being at the pinned commit, as the prompt said.

3. **Next time:** I'd add `rg -n` for `createRedirectRenderResult` and `x-action-redirect`. I'd also use `git -C … rev-parse HEAD` to verify the pin, and print `redirect.ts` with line numbers (`rg -n ''`) so every citation is exact.

4. **Confidence:** High for the throw mechanism and the route-handler, render and server-action MPA paths, because I read that code directly. Medium for the fetch-action detail, since I didn't read it.