# Q5 — Vue PR #15035

PR [#15035](https://github.com/vuejs/core/pull/15035) fixes at least these hydration/interoperability cases:

1. A hydrated `Suspense` branch containing an unresolved async component can be unmounted, remounted, and then resolved without reviving/displacing stale SSR DOM. The added runtime-core test explicitly unmounts the hydrated branch before the client promise resolves and checks the later result.
2. The corresponding async-`setup`-under-`Suspense` case can be unmounted before it resolves; late resolution must leave the fallback DOM intact. That is a separate added runtime-core test in the PR diff.

In `runtime-core`, the patch records a placeholder subtree for an unresolved async wrapper or `asyncDep`, and removes that adopted SSR DOM if the component is unmounted before its effect exists; see the changed [`hydration.ts`](https://github.com/vuejs/core/pull/15035/files#diff-5) and [`renderer.ts`](https://github.com/vuejs/core/pull/15035/files#diff-6) patches. `runtime-vapor` needs its own changes because its VDOM interop owns Vapor blocks, anchors, adopted ranges, and slot content rather than using only the ordinary renderer: the diff preserves the hydration anchor, records `vnode.el` for unresolved async Vapor components, and removes the adopted range/anchors through its interop cleanup ([PR files](https://github.com/vuejs/core/pull/15035/files)). Those are distinct ownership models, so changing only runtime-core would still leave Vapor's range/anchor bookkeeping inconsistent.
