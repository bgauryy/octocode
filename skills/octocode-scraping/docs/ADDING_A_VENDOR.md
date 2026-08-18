# Adding a Scraping Vendor

This skill's corpus, extraction, and graph-building logic (`scripts/lib/corpus.mjs`, `analyzers.mjs`, `extractors.mjs`) never import a vendor client or branch on a provider's name. They only consume one generic shape — `FetchResponse`. Adding a vendor means writing one function and registering it; nothing else in the skill changes.

## The contract

Defined formally in `schemas/provider.schema.json` (`$defs.FetchResponse` and `$defs.ProviderDescriptor`).

**`FetchResponse`** — what your vendor's fetch function must return:

```js
{
  pageId,        // passed in, echo back
  url,           // passed in, echo back
  status,        // integer HTTP status
  contentType,   // string, may be ''
  body,          // string — raw text; JSON-envelope or raw HTML/markdown, your choice
  fetchError,    // string | null — set on network/transport failure, otherwise null
  creditCost,    // string | number | null — vendor-reported per-request cost, or null if the vendor has none
  fetchedAt      // ISO 8601 timestamp string
}
```

**`ProviderDescriptor`** — the registry entry describing your vendor:

```js
{
  name,             // string, matches the registry key
  fetch,            // async ({ url, pageId, config, apiKey }) => FetchResponse
  supportsModes,    // subset of ['html', 'markdown', 'extended', 'extract']
  requiresApiKey,   // boolean
  apiKeyEnv         // env var name to read the key from, or null when requiresApiKey is false
}
```

## Steps

1. **Implement the fetch function** in `scripts/lib/client.mjs`, following the existing `fetchScrapingAnt` / `fetchDirect` functions as templates. Reuse the existing mock plumbing (`config.mockStatus` / `config.mockBodyFile` / `config.mockContentType`) so your vendor is testable without a real network call or key.
2. **Register it** in `scripts/lib/providers.mjs`'s `PROVIDERS` map with its `ProviderDescriptor`.
3. **Verify**: check every registered descriptor against the schema, reject unknown providers and unsupported provider/mode pairs, and confirm that a fetch through your new provider produces the *same corpus shape* (`AGENT_INDEX.json`, `graph/graph.json`, workflow classification, etc.) as every other provider. That parity check is the actual proof the abstraction held — if it breaks, something leaked a vendor assumption into shared code.
4. Run `node scripts/skill-review.mjs` from `octocode-skills` before calling it done.

## Worked example: a hypothetical `firecrawl` provider

```js
// scripts/lib/client.mjs
export async function fetchFirecrawl({ url, pageId, config, apiKey }) {
  // ... call Firecrawl's API, or honor config.mockStatus for tests ...
  return { pageId, url, status, contentType, body, fetchError, creditCost, fetchedAt: new Date().toISOString() };
}
```

```js
// scripts/lib/providers.mjs
import { fetchFirecrawl } from './client.mjs';

export const PROVIDERS = {
  scrapingant: { /* ... */ },
  direct: { /* ... */ },
  firecrawl: { name: 'firecrawl', fetch: fetchFirecrawl, supportsModes: ['html', 'markdown'], requiresApiKey: true, apiKeyEnv: 'FIRECRAWL_API_KEY' }
};
```

That's it — `--provider firecrawl` now works end to end: corpus building, graph/workflow analysis, and every search helper script apply unchanged, because none of them ever looked at which vendor produced the page.

## What "mode" means per vendor

`html`/`markdown`/`extended`/`extract` are *capabilities*, not universal guarantees. `direct` (plain HTTP) can only ever support `html` — there's no vendor to do markdown conversion or AI extraction. Declare only the modes your vendor actually implements in `supportsModes`; the arg parser (`scripts/lib/args.mjs`) rejects any `--mode` your provider doesn't declare, with a clear error naming what it does support.

## See also

- `references/providers.md` — agent-facing routing: when to load this doc, how the registry is chosen at runtime.
- `docs/PROVIDERS.md` — how to configure vendor API keys (`~/.octocode/.env`) and where the MCP server fits in.
- `schemas/provider.schema.json` — the formal contract both structures above must satisfy.
