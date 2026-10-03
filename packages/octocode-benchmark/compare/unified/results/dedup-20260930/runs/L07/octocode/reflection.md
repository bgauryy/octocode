**Helped:**
- The first `localSearch` for `isRedirectError` under `server/` was the fastest step. It returned every server-side catch site in one call: `app-render.tsx`, `action-handler.ts`, `make-get-server-inserted-html.tsx` and `app-route/module.ts`.
- The parallel `localFetch` of `redirect.ts` (full content) gave the throw side and the digest format.
- The three line-range `localFetch` calls at the hit lines confirmed the catch behaviour.

**Did not help:**
- Some windows I chose were too short. The `module.ts` read ended mid-expression at `status: actionStore.isAction`, so I never saw the non-action status. The `app-render.tsx` 4375-4400 read also stopped before the headers were set.
- I never opened `action-handler.ts`, `make-get-server-inserted-html.tsx`, or `redirect-status-code`, so I left the 303/307/308 values, the action path and the meta-tag emission unverified.
- I never used `lspSearch` or `astSearch`.

**Next time:** I would read about 30 lines further past each hit. I would also do a second round of parallel reads on the files I skipped. I'd use `matchString` instead of guessed line ranges.

**Confidence:** Medium-high on the core flow (throw a digest error, catch it, set status and `Location`). Medium on details for the paths I did not read.