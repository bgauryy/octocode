**Short answer:** `redirect()` and `permanentRedirect()` don't return a response. They throw a special error, and a server-side catch block turns that error into a status code and a `Location` header. Which catch block runs depends on the context: page render, route handler, or server action. All paths below are under `packages/next/src`.

**1. Throwing the error**
- `redirect()` (`client/components/redirect.ts:~38`) calls `getRedirectError(url, type, 307)`. `type` defaults to `'push'` inside a server action and `'replace'` elsewhere.
- `permanentRedirect()` (`client/components/redirect.ts:~58`) does the same with status 308 and a `'replace'` type by default.
- `getRedirectError` (`client/components/redirect.ts:9-17`) creates `new Error('NEXT_REDIRECT')`. It sets `error.digest = "NEXT_REDIRECT;<type>;<url>;<status>;"`, and both functions then `throw` it.
- `isRedirectError` (`client/components/redirect-error.ts:18-42`) recognises the error by parsing that digest. It checks the error code, that the type is `push` or `replace`, and that the status is a valid `RedirectStatusCode`.
- `getURLFromRedirectError` and `getRedirectStatusCodeFromError` (`client/components/redirect.ts`) pull the URL and status back out of the digest.

**2. Catching it and building the response**
- **Page render (Server Components):** the catch blocks in `server/app-render/app-render.tsx` do this.
  - At about lines 4386-4402, `isRedirectError(err)` leads to `res.statusCode = getRedirectStatusCodeFromError(err)`. The URL is prefixed with `basePath` through `addPathPrefix`, any mutable cookies are copied into `set-cookie`, and `setHeader('location', redirectUrl)` is called.
  - A second path, at about lines 9833-9846, sets the status and `location` header the same way. It doesn't set cookies.
  - If the response has already started streaming, the status and headers can't change. In that case `server/app-render/make-get-server-inserted-html.tsx:56-72` injects `<meta id="__next-page-redirect" http-equiv="refresh" content="0;url=…">`. It uses `0` for a 308 and `1` otherwise.
- **Route handlers:** `server/route-modules/app-route/module.ts:424-449` returns `new Response(null, { status, headers })`. The headers contain `Location` plus any cookies set through the cookies API. The status is 303 if the call happened inside a server action, and otherwise the digest's status (307 or 308).
- **Server actions:** `server/app-render/action-handler.ts:1324-1360` handles it.
  - For a fetch action (JS enabled), the HTTP status stays 200. The redirect travels in the `x-action-redirect` header, and the body can carry the target's Flight response through `createRedirectRenderResult`. I did not open that function to confirm the header name.
  - For a no-JS form post (MPA), it sets status 303 and a `Location` header, and sends an empty body (`RenderResult.EMPTY`).

**Uncertainty:** I read the code only through the catch sites above. I did not trace the callers that wrap these handlers, or the client-side handling of the redirect.