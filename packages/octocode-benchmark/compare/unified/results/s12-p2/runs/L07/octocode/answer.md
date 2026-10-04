`redirect()` and `permanentRedirect()` don't send a response themselves. They throw a tagged error, and whichever server layer is running catches it and writes the status code and `Location` header. I read all of this in the local checkout. I did not trace the Flight stream or the `RedirectStatusCode` values, and I did not check the `app-render.tsx` line numbers beyond the ranges quoted below.

**1. Throwing the error** (`packages/next/src/client/components/redirect.ts`)
- `getRedirectError` (lines 10-18) creates `new Error('NEXT_REDIRECT')`. It sets `error.digest = "NEXT_REDIRECT;<type>;<url>;<statusCode>;"`.
- `redirect()` (lines 33-41) throws that error with `RedirectStatusCode.TemporaryRedirect`, which is 307. The type defaults to `'push'` inside a server action and `'replace'` otherwise.
- `permanentRedirect()` (lines 55-61) throws it with `PermanentRedirect`, which is 308. Its type defaults to `'replace'`.
- `redirect-error.ts:18-42` has `isRedirectError`, which recognises the error by checking the digest format.
- `redirect.ts:70-92` has helpers that read the URL, type and status back out of the digest.

**2. Catching it and building the response**

- **Page render (RSC and SSR)**, `server/app-render/app-render.tsx`:
  - Lines 4386-4403 (error recovery): if `isRedirectError(err)`, it sets `res.statusCode = getRedirectStatusCodeFromError(err)`. It then sets `location` to `addPathPrefix(getURLFromRedirectError(err), basePath)`. Any mutable cookies are copied into `set-cookie` first.
  - Lines 9833-9846 do the same in the second error-recovery path.
  - The recovery code then renders an error shell. The status and `Location` header are already set, so the browser gets a 307 or 308.
  - `make-get-server-inserted-html.tsx:56-58` also handles the redirect error. I did not read it. By the docs in `redirect.ts`, it is the part that inserts the meta-refresh tag for the HTML body.
- **Route handlers**, `server/route-modules/app-route/module.ts`, `handleHandlerError`:
  - Lines 424-449 return `new Response(null, { status, headers })` with `Location: url` and any mutable cookies appended.
  - The status is 303 if `actionStore.isAction`, otherwise the status from the digest (307 or 308).
- **Server actions**, `server/app-render/action-handler.ts:1324-1357`:
  - A fetch action (client JavaScript) gets status 200 (lines 1328-1347). `createRedirectRenderResult` builds the redirect, which the client reads from the `x-action-redirect` header per the comment at line 1329. The redirect is not sent as an HTTP redirect.
  - A no-JavaScript form post (MPA action) gets 303 with a `Location` header and an empty body (lines 1350-1357).