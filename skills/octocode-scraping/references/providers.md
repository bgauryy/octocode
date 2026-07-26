# Providers

Load when choosing `--provider`, checking route capabilities, or adding a vendor. Why: fetching may vary, but corpus/extraction logic must stay vendor-agnostic.

## Contract
Each provider is one `fetch({ url, pageId, config, apiKey })` returning `FetchResponse` (`scripts/schemas/provider.schema.json`). `corpus.mjs`, `analyzers.mjs`, and `extractors.mjs` consume only that shape; they must not import vendor clients or branch on provider names.

## Registry
`PROVIDERS` in `scripts/lib/providers.mjs` is the only vendor list: `name`, `fetch`, `supportsModes`, `requiresApiKey`, `apiKeyEnv`.

| Provider | Modes | Key | Best for |
|---|---|---|---|
| `direct` | html | no | Static pages, tests, no vendor/no key. |
| `cdp` | html | no | JS-rendered pages without a hosted key; requires sibling browser skill + Chrome. |
| `scrapingant` | html, markdown, extended, extract | yes (`SCRAPING_ANT`) | Anti-bot, hosted rendering, markdown, AI extraction, extended rows. |

## Capability matrix
| Need | Route |
|---|---|
| Plain static HTML | `direct` |
| Real browser render, keyless | `cdp` |
| Markdown output | `scrapingant` |
| Extended rows (XHR/cookies/iframes) | `scrapingant` |
| AI structured extraction | `scrapingant` |
| Lowest cost | `direct`, then `cdp` |
| Highest resilience | `scrapingant`, unless live/auth state requires `octocode-chrome-devtools` |

## Auto-selection
Omit `--provider` to use priority order:
1. `scrapingant` if `SCRAPING_ANT` is configured.
2. `cdp` if sibling browser skill is installed.
3. `direct` fallback.

Run `scripts/provider-check.mjs` to see the active route. Explicit `--provider` overrides auto-selection. Non-html modes require `scrapingant`.

## Add a vendor
1. Add `fetchX(...)` in `scripts/lib/client.mjs` returning `FetchResponse`.
2. Register descriptor in `PROVIDERS`.
3. Run `scripts/eval-providers.mjs` to validate descriptor schema and same corpus shape.

No changes to corpus/analyzer/extractor modules should be needed. For worked setup use `docs/ADDING_A_VENDOR.md` and `docs/PROVIDERS.md`.
