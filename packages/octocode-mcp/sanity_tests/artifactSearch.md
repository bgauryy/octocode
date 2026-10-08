# Sanity check — `artifactSearch`

Use the live MCP tool or built CLI. Read the schema first; keep these checks aligned with the canonical core contract.

## Contract

- [ ] Catalog exposes `artifactSearch` and no obsolete package-tool alias.
- [ ] Each query requires one ecosystem `type`: npm, pypi, crates, maven, nuget, go, packagist, or rubygems.
- [ ] Exactly one selector: exact `packageName` or non-empty `keywords` array.
- [ ] Discovery accepts `cursor` and `pageSize` (default 10, maximum 100); exact reads reject both. Numbered `page` is rejected.
- [ ] `registryUrl` is npm-only; secrets do not appear in query examples or output.
- [ ] PyPI keyword discovery returns a typed unsupported error and a concise exact-lookup hint.

## Pagination and output

- [ ] Execute each `next.nextPage` verbatim; compare the union with the complete fixture, including mismatched provider/tool page sizes.
- [ ] Continuations preserve ecosystem, selector, page size, registry, and provider state. Empty provider pages with continuation remain reachable.
- [ ] Unknown totals remain unknown; terminal limits are explicit.
- [ ] Response-size pagination also exposes executable continuation; no silent clipping.
- [ ] `artifacts[]` retains canonical identity, registry URL, and available metadata; Go package paths stay separate from module identities.
- [ ] Success has no advisory hints. Empty/error results have concise recovery; pagination/completeness calls remain available.

## Provider quality

- [ ] Exact positive/missing identities work for all eight ecosystems; scoped npm and Maven coordinates stay intact.
- [ ] Supported keyword discovery yields relevant candidates and usable source metadata where supplied.
- [ ] Authentication failures, throttling, and provider failures stay errors, distinct from a missing package.
- [ ] npm scoped routing, explicit registry overrides, credentials, and cache isolation still work.
- [ ] Package source links lead to evidence; they do not establish installed-version or implementation equivalence.

## Example

```json
{"queries":[{"ecosystem":"npm","packageName":"react"},{"ecosystem":"pypi","packageName":"requests"},{"ecosystem":"crates","keywords":["async","runtime"],"pageSize":2}]}
```
