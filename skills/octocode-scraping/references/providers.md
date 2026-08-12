# Providers

Load when choosing `--provider`, checking routes, or adding a vendor. Why: fetch may vary; corpus stays vendor-agnostic. Decision tree: `route-selection.md`.

## Contract
`fetch({ url, pageId, config, apiKey })` → `FetchResponse` (`scripts/schemas/provider.schema.json`). Corpus/analyzers must not branch on vendor names.

| Provider | Modes | Key | Best for |
|---|---|---|---|
| `direct` | html | no | Static / cheapest proof |
| `cdp` | html | no | Local JS render (sibling chrome-devtools) |
| `scrapingant` | html, markdown, extended, extract | `SCRAPING_ANT` | Hosted anti-bot / markdown / extract — **explicit only** |

Omit `--provider` on html → `cdp` if chrome skill present, else `direct`. Hosted never auto. Check: `provider-check.mjs`.

## Add a vendor
Add `fetchX` in `lib/client.mjs` → register in `PROVIDERS` → `eval-providers.mjs`. Setup: `docs/PROVIDERS.md`, `docs/ADDING_A_VENDOR.md`.
