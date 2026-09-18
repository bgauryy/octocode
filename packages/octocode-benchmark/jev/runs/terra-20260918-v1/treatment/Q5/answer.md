# Q5 — Vue PR #15035

[`vuejs/core#15035`](https://github.com/vuejs/core/pull/15035) fixes, among other cases, (1) hydrating a Suspense branch containing an unresolved async component/async setup component and then unmounting it before resolution, so late resolution does not restore stale SSR DOM; and (2) Vapor/VDOM interop around dynamic-component anchors and slot fallback/parked invalid content, avoiding mismatched or wrongly retained DOM ranges.

Both packages are necessary because `runtime-core` owns standard VDOM hydration, placeholder construction, and renderer removal, while `runtime-vapor` owns Vapor block boundaries, component unmount semantics, and the VDOM-interoperability bridge/anchors. The PR diff changes both implementation paths and adds corresponding regression tests. The retrieved patch packet was provider-truncated, so this is a bounded review of the directly returned changes.
