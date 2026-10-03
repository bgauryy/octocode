`redirect()` and `permanentRedirect()` never write an HTTP response themselves. They throw a tagged error, and whichever server layer catches it turns it into a status code and `Location` header. I did not check `redirect-error.ts` (for `isRedirectError` and `REDIRECT_ERROR_CODE`) or `redirect-status-code.ts` (for the enum values).

**1. Throwing the error** (`packages/next/src/client/components/redirect.ts`)
- `getRedirectError` (lines 9–17) creates `new Error(REDIRECT_ERROR_CODE)` and sets `error.digest = \`${REDIRECT_ERROR_CODE};${type};${url};${statusCode};\``.
- `redirect()` throws it with `RedirectStatusCode.TemporaryRedirect` (307). The type defaults to `'push'` inside a server action and `'replace'` elsewhere.
- `permanentRedirect()` throws it with `RedirectStatusCode.PermanentRedirect` (308). The type defaults to `'replace'`.
- Helpers read the digest back: `getURLFromRedirectError`, `getRedirectTypeFromError` and `getRedirectStatusCodeFromError`. The status code is the second-to-last `;` field.

**2. Catching it, which depends on the context**
- **Page / Server Component render** (`server/app-render/app-render.tsx`):
  - The `isRedirectError(err)` branch at ~4386–4403 sets `res.statusCode = getRedirectStatusCodeFromError(err)`. It then sets `location` to `addPathPrefix(getURLFromRedirectError(err), basePath)`, and copies any mutable cookies into `set-cookie`.
  - A second, similar branch at ~9839–9846 sets the status and `location` the same way. It has no cookie copying in the lines I read.
  - After that, an error shell is rendered with this status and header.
  - If the redirect happens after the response has started streaming, the status and headers are already sent. In that case `server/app-render/make-get-server-inserted-html.tsx:56–73` injects `<meta id="__next-page-redirect" http-equiv="refresh" content="0;url=…">` instead. It uses `0` for permanent and `1` for temporary redirects.
- **Route Handlers** (`server/route-modules/app-route/module.ts:424–449`): the handler's error path builds `new Response(null, { status, headers })` with a `Location` header and any mutable cookies. The status is 303 (`SeeOther`) if `actionStore.isAction`, otherwise `getRedirectStatusCodeFromError(err)` (307 or 308).
- **Server Actions** (`server/app-render/action-handler.ts:1324–1357`):
  - For a fetch action, it returns a 200 response. The redirect goes through the `x-action-redirect` mechanism via `createRedirectRenderResult`, which can include the target's Flight payload.
  - For a progressive-enhancement form post (MPA), it sets status 303 (`RedirectStatusCode.SeeOther`) and a `Location` header, with an empty body (`RenderResult.EMPTY`).

**Uncertainty:** I only read the windows quoted above. I did not trace how a redirect thrown from middleware or from `next.config` `redirects` is handled, because those take different paths. I also did not check what `x-action-redirect` does in `createRedirectRenderResult`.