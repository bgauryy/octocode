# Q1 — Next.js route regex

On `canary`, `getRouteRegex()` is exported by [`packages/next/src/shared/lib/router/utils/route-regex.ts`](https://github.com/vercel/next.js/blob/3bf71ee3fd1f55fbe0f53956973de64b85b2bdf9/packages/next/src/shared/lib/router/utils/route-regex.ts#L149-L169). It calls the internal `getParametrizedRoute()` helper, which returns `parameterizedRoute` and `groups` ([lines 69–115](https://github.com/vercel/next.js/blob/3bf71ee3fd1f55fbe0f53956973de64b85b2bdf9/packages/next/src/shared/lib/router/utils/route-regex.ts#L69-L115)).

The top-level result fields of `getRouteRegex()` are `re` (a `RegExp`) and `groups`; the optional trailing slash is appended before constructing `re` ([lines 149–169](https://github.com/vercel/next.js/blob/3bf71ee3fd1f55fbe0f53956973de64b85b2bdf9/packages/next/src/shared/lib/router/utils/route-regex.ts#L149-L169)).
