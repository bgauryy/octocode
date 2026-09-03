# Add a scraping vendor

Vendor code returns one `FetchResponse`; corpus, extraction, and graph modules remain provider-independent. Formal shapes live in `scripts/schemas/provider.schema.json`.

## Contracts

```js
// FetchResponse
{ pageId, url, status, contentType, body, fetchError, creditCost, fetchedAt }

// ProviderDescriptor
{ name, fetch, supportsModes, requiresApiKey, apiKeyEnv }
```

`fetch` is `async ({ url, pageId, config, apiKey }) => FetchResponse`. Echo `pageId`/`url`; use integer HTTP `status`, string `contentType`/`body`, `null` or a message for `fetchError`, vendor cost or `null` for `creditCost`, and ISO-8601 `fetchedAt`. `supportsModes` is a subset of `html`, `markdown`, `extended`, and `extract`.

## Add and verify

1. Add `fetchX` to `scripts/lib/client.mjs`. Preserve `config.mockStatus`, `mockBodyFile`, and `mockContentType` so tests need neither network nor key.
2. Register its descriptor in `scripts/lib/providers.mjs`.
3. Validate every descriptor, unknown-provider handling, and provider/mode rejection against the schema.
4. Compare a mock fetch with existing providers: it must produce the same `AGENT_INDEX.json`, graph, workflow, and extraction shapes.
5. Run the `octocode-skills` review against this folder.

Example registration:

```js
import { fetchFirecrawl } from './client.mjs';

export const PROVIDERS = {
  direct: { /* existing */ },
  firecrawl: {
    name: 'firecrawl', fetch: fetchFirecrawl,
    supportsModes: ['html', 'markdown'],
    requiresApiKey: true, apiKeyEnv: 'FIRECRAWL_API_KEY'
  }
};
```

Declare only implemented modes: `direct` supports HTML only, and `scripts/lib/args.mjs` rejects undeclared combinations. Setup lives in `docs/PROVIDERS.md`; agent routing in `references/providers.md`.
