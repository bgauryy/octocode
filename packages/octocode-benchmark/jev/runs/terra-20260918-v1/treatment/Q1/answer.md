# Q1 — Next.js route-regex result

On `canary`, the exported function is in [`packages/next/src/shared/lib/router/utils/route-regex.ts`](https://github.com/vercel/next.js/blob/canary/packages/next/src/shared/lib/router/utils/route-regex.ts). `getRouteRegex()` calls the internal `getParametrizedRoute(normalizedRoute, includeSuffix, includePrefix)` helper. Its top-level return object has exactly `re` (a `RegExp`) and `groups`.

Evidence: the retrieved canary source resolved to `3bf71ee3fd1f55fbe0f53956973de64b85b2bdf9` and shows both that call and `return { re: new RegExp(...), groups: groups }`.

The forced Jev probe suggested checking for a type-level alternative; the bounded search did not surface one, and it does not override the direct return literal.
