# Q2 — `sindresorhus/is`

The repository is [`sindresorhus/is`](https://github.com/sindresorhus/is). Its discovery record describes it as “Type check values” and reports TypeScript; the tree metadata confirms TypeScript as dominant and `main` as the default branch.

Bounded answer: **No**—the public root surface examined does not define or export `isQuantumSuperposition`. A scoped indexed-code search returned no match (explicitly marked unproven absence), then the direct `main` checks found no such name in [`source/index.ts`](https://github.com/sindresorhus/is/blob/main/source/index.ts). [`package.json`](https://github.com/sindresorhus/is/blob/main/package.json) maps the root `exports` to generated `distribution/index.d.ts`/`distribution/index.js`. That is bounded evidence, not a claim that an unfetched generated artifact can never differ.
