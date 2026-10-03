**Short answer:** `redirect()` and `permanentRedirect()` don't return a response. They throw a special error whose `digest` string holds the redirect details. The server code that catches errors recognises it and sets the status code and `Location` header. What it does differs by context: page render, route handler, or server action.

**1. Throwing the error** (`packages/next/src/client/components/redirect.ts`)
- `getRedirectError` (lines 9-17) creates `new Error('NEXT_REDIRECT')`. It sets `digest = "NEXT_REDIRECT;<type>;<url>;<status>;"`.
- `redirect()` throws it with status 307 (`TemporaryRedirect`). Its type defaults to `push` inside a server action and `replace` otherwise (lines 38-45).
- `permanentRedirect()` throws it with status 308 (`PermanentRedirect`), and its type defaults to `replace` (lines 62-67).
- `isRedirectError` (`redirect-error.ts:15-40`) checks the digest format. Helpers in `redirect.ts` (`getURLFromRedirectError`, `getRedirectStatusCodeFromError`) parse the URL and status back out of it.

**2. Catching it and building the response**
- **Page and RSC render** (`server/app-render/app-render.tsx`):
  - Around lines 4386-4400 and 9840-9846, the error handler checks `isRedirectError(err)`.
  - It sets `res.statusCode = getRedirectStatusCodeFromError(err)` and `metadata.statusCode`.
  - It then calls `setHeader('location', addPathPrefix(url, basePath))`. The first path also copies mutable cookies into `set-cookie`.
  - The error is also captured so the HTML stream gets a fallback tag. `make-get-server-inserted-html.tsx:56-72` injects `<meta id="__next-page-redirect" http-equiv="refresh" content="0|1;url=...">`. The delay is 0 for permanent redirects and 1 for temporary ones.
  - This matters for streaming. If the status line has already been sent, the meta tag is the only redirect signal left.
- **Route handlers** (`server/route-modules/app-route/module.ts:424-449`):
  - It returns `new Response(null, { status, headers })` with `Location` set to the URL, plus any cookies set through the cookie API.
  - The status is 303 (`SeeOther`) if the handler ran as an action. Otherwise it is the digest's status (307 or 308).
- **Server actions** (`server/app-render/action-handler.ts:1324-1355`):
  - For an MPA, non-fetch form POST, it sets status 303 and the `Location` header, and returns an empty body (`RenderResult.EMPTY`).
  - For a fetch action, it keeps status 200 and calls `createRedirectRenderResult`. The redirect is carried in the `x-action-redirect` header, which I read in a code comment but did not trace through that function. The body can include the target's Flight payload.

**Not verified:** I didn't read `createRedirectRenderResult` or the client-side handling of redirects. I also didn't check whether other callers, such as the prerender or static-generation paths, handle redirect errors separately from the ones above.