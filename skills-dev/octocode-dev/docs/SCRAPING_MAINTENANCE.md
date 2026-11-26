Load when changing the `skills/octocode-scraping` scripts or their tests, or adding a vendor.

# Maintaining octocode-scraping

Usage lives in the skill's `SKILL.md` and `references/`; script inputs in each script's `--help`. Paths below are under `skills/octocode-scraping/scripts/`.

## Layout

- Entry scripts at the top; shared modules in `lib/`.
- `schemas/graph.schema.json` is copied into every session (`lib/corpus.mjs`) for downstream tools. No JSON-schema validator runs in the skill or its tests: schemas and the vendor contract are enforced only by tests.
- Errors go through `lib/fail.mjs` in the shape of `skills/octocode-scraping/references/script-output.md#stdout-contract`; `corpus-inspect`, `dom-find`, `graph-navigate` and `resource-list` print usage on stderr instead.

## Vendor contract

Vendor code returns one `FetchResponse`; corpus, extraction and graph code (`lib/corpus.mjs`, `analyzers.mjs`, `extractors.mjs`) consume only that and stay provider-independent.

```js
// FetchResponse: all fields required
{ pageId, url, status, contentType, body, fetchError, creditCost, fetchedAt }

// ProviderDescriptor: an entry in PROVIDERS (lib/providers.mjs)
{ name, fetch, supportsModes, requiresApiKey, apiKeyEnv, cleanup? }
```

- `fetch`: `async ({ url, pageId, config, apiKey }) => FetchResponse`, echoing `pageId` and `url`. `status` is an integer; `body` the raw response text (a vendor JSON envelope as a string, or raw HTML or markdown); `fetchError` `null` or a message; `creditCost` the vendor's per-request cost, or `null` for a keyless direct fetch; `fetchedAt` ISO-8601.
- `supportsModes`: only implemented modes from `html`, `markdown`, `extended`, `extract` (`direct` is HTML only); `lib/args.mjs` rejects an undeclared provider and mode pair.
- `apiKeyEnv`: the key's variable, `null` when none is needed. `cleanup(config)`: called once after the fetch or crawl loop, for a provider holding a resource such as a launched browser (`cdp`).

## Adding a vendor

1. Add `fetchX` to `lib/client.mjs`, honoring `config.mockStatus`, `mockBodyFile` and `mockContentType` (`--mock-status`, `--mock-body-file`, `--mock-content-type`) so tests need no network or key.
2. Register it in `PROVIDERS`, e.g. `firecrawl: { name: 'firecrawl', fetch: fetchFirecrawl, supportsModes: ['html', 'markdown'], requiresApiKey: true, apiKeyEnv: 'FIRECRAWL_API_KEY' }`.
3. Test the descriptor, unknown-provider handling and provider/mode rejection; a mock fetch must produce the same `AGENT_INDEX.json`, graph, workflow and extraction shapes as existing providers.

## Tests

- One file: `node --test skills/octocode-scraping/scripts/tests/<file>.test.mjs`; all: `node --test 'skills/octocode-scraping/scripts/tests/*.test.mjs'` (a bare directory fails). `$DEV skills:check` runs them in CI.
- A browser integration change also needs a live browser check.
- Keep covered: compact stdout with no raw payload; agent index; graph v2 with source evidence on nodes and edges; target-error warnings; secret rejection without a stack trace; cost capture; failure reports; resource rows without `workflowType`; bridge `thinHints`, value-level redaction, rerun replacement, and API-field proof by local regex without live Chrome.
- Finish with `node skills/octocode-skills/scripts/skill-review.mjs skills/octocode-scraping`.
