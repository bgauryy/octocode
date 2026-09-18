# Q7 — Zustand Next.js contract

[`store.ts`](https://github.com/vercel/next.js/blob/canary/examples/with-zustand/src/lib/store.ts) is a React Context-backed store factory, not a module singleton: it uses React `createContext`/`useContext`, Zustand `createStore`/`useStore`, and `initializeStore` returns a new store. [`StoreProvider.tsx`](https://github.com/vercel/next.js/blob/canary/examples/with-zustand/src/lib/StoreProvider.tsx) creates it in a `useRef` when the provider has none and supplies it via `Provider`.

In Zustand's root [`package.json`](https://github.com/pmndrs/zustand/blob/main/package.json), `react` is in `peerDependencies`, and `peerDependenciesMeta.react.optional` is `true`; React is therefore an optional peer, not a required installed dependency.
