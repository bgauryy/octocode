1. **Helped:** The first batch was the fastest step. `localSearch` for `getRedirectStatusCodeFromError|isRedirectError|getURLFromRedirectError` under `src/server` returned every consumer file with context lines. Fetching `redirect.ts` in full in the same batch gave the digest format and the throw sites. The follow-up `localFetch` calls with line ranges (app-render 4375-4410 and 9815-9860, app-route `module.ts` 420-452) confirmed the status-code and `Location` header logic.

2. **Did not help:** Nothing errored and I made no repeated calls. I did not use `lspSearch`, so I never traced call paths from the catch sites back toward React's error handling. The `localSearch` output is verbose, with duplicated context blocks.

3. **Next time:** I would read `redirect-error.ts`, which I never opened, to confirm the digest check and the status values. I would also read past `action-handler.ts:1324` and trace what calls the app-render catch blocks.

4. **Confidence:** Medium-high on the core mechanism: the thrown digest, the catch in app-render that sets status and `Location`, and the app-route `Response`. Two points are weaker:
   - I said the meta-refresh tag is for streams that have already started. The file only shows it being built, so that explanation is my inference.
   - The 307/308 values come from doc comments, not from `RedirectStatusCode` itself.