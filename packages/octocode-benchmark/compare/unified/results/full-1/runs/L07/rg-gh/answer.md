**Short answer:** `redirect()` and `permanentRedirect()` don't return anything. They throw a special `Error`, and the server code that catches it turns it into a status code and a `Location` header. What that code does depends on where the call happened: a page render, a route handler, or a server action. All paths below are under `packages/next/src/`.

**1. The throw**
- `client/components/redirect.ts:9-17`: `getRedirectError` builds `new Error('NEXT_REDIRECT')`. It sets `error.digest = "NEXT_REDIRECT;<type>;<url>;<status>;"`.
- `redirect()` throws with status 307, `TemporaryRedirect`. Its `type` defaults to `push` inside a server action and `replace` otherwise (`redirect.ts:36-43`).
- `permanentRedirect()` throws with status 308, `PermanentRedirect`, and `type` defaults to `replace` (`redirect.ts:61-66`).
- `client/components/redirect-error.ts:14-40`: `isRedirectError` recognises the error by parsing its digest.
- `redirect.ts:75-97`: `getURLFromRedirectError`, `getRedirectTypeFromError` and `getRedirectStatusCodeFromError` read the URL, type and status back out of the digest.

**2. Page and RSC render (`server/app-render/app-render.tsx`)**
- Around line 4386 (the render-error handler): `isRedirectError(err)` is true. The code sets `res.statusCode = getRedirectStatusCodeFromError(err)`. It computes `addPathPrefix(getURLFromRedirectError(err), basePath)`. It copies any mutable cookies into `set-cookie`, then calls `setHeader('location', redirectUrl)` (lines 4386-4404).
- Around line 9833 there is a second copy of the same logic. It sets the status and the `location` header, and does not copy cookies (lines 9841-9846).
- When the redirect is discovered after the response has started streaming, a `<meta http-equiv="refresh">` tag is injected instead. The delay is 0 for a permanent redirect and 1 for a temporary one (`server/app-render/make-get-server-inserted-html.tsx:56-72`). Because the redirect is an error, the page still renders its error shell after the status and header are set.

**3. Route handlers (`server/route-modules/app-route/module.ts:424-450`)**
- The catch block builds `new Response(null, { status, headers })` with `Location: url` and any mutable cookies.
- The status is 303 (`SeeOther`) if `actionStore.isAction` is true. Otherwise it is the digest's status, 307 or 308.

**4. Server actions (`server/app-render/action-handler.ts:1324-1360`)**
- A fetch action (the client-side call) is not sent as an HTTP redirect. It returns status 200, and the redirect goes through the `x-action-redirect` mechanism. The body is built by `createRedirectRenderResult`, which can include the target's Flight response.
- A no-JS form post (MPA action) gets `res.statusCode = 303`, a `Location` header, and an empty body (`RenderResult.EMPTY`).

**Uncertainty**
- I read the catch sites above but did not trace the `x-action-redirect` header assignment inside `createRedirectRenderResult`.
- I did not check whether `permanentRedirect()` (308) is downgraded to 303 in every action context. The route-handler and MPA-action code shows 303 in both.
- I did not trace how `metadata.statusCode` is used downstream.