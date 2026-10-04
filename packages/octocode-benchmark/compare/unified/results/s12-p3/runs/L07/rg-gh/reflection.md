1. **Helped:** The first Bash call (`cat client/components/redirect.ts`) gave the throw side right away. The second, a `grep -rnE` for `isRedirectError|getURLFromRedirectError|getRedirectStatusCodeFromError` under `server/`, found every catch site. The third `sed` call then read those sites in one batch.

2. **Did not help:** `rg` wasn't installed, so that half of the first call failed and I fell back to grep. Every Bash call printed `/dev/null: Operation not permitted`. It was harmless but noisy. I never opened `createRedirectRenderResult` to confirm the `x-action-redirect` header, and I never checked the callers around these handlers.

3. **Next time:** I'd use grep from the start. I'd read `createRedirectRenderResult` in a fourth call. I'd also state the line numbers more carefully. Several were approximate (`~38`, `~58`), and the `sed` output didn't show line numbers, so those were estimates from the ranges I read. I'd run `grep -n` or `nl` on the exact spans.

4. **Confidence:** Medium-high. The core mechanism (throw, then catch, then status plus `Location`) is directly visible in the code I read. The `x-action-redirect` detail and a few line numbers are less solid.