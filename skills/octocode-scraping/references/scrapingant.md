# ScrapingAnt

Load when using `scripts/scrapingant-fetch.mjs` or the ScrapingAnt API. Why: keep auth, endpoint, and output behavior deterministic.

## Auth and endpoints
- Env key: `SCRAPING_ANT`; load through `@octocodeai/config`.
- Never print the key or embed it in logs.
- `--mode html`: `https://api.scrapingant.com/v2/general`.
- `--mode markdown`: `https://api.scrapingant.com/v2/markdown`.
- `--mode extended`: `https://api.scrapingant.com/v2/extended` writes headers/XHRs/iframes/cookies-redacted extracts.
- `--mode extract --extract-properties <text>`: `https://api.scrapingant.com/v2/extract` writes `extracts/{pageId}-ai-extract.json`.
- Usage endpoint script: `scripts/scrapingant-usage.mjs` calls `/v2/usage` and prints sanitized plan/credit fields.
- Required query params: `url`, `x-api-key`; extract additionally requires `extract_properties`.

## Script examples
```bash
node skills/octocode-scraping/scripts/scrapingant-check.mjs
node skills/octocode-scraping/scripts/scrapingant-fetch.mjs --url https://example.com --mode html
node skills/octocode-scraping/scripts/scrapingant-fetch.mjs --url https://example.com --mode markdown
node skills/octocode-scraping/scripts/scrapingant-fetch.mjs --url https://example.com --mode extended
node skills/octocode-scraping/scripts/scrapingant-fetch.mjs --url https://example.com --mode extract --extract-properties "title, content"
node skills/octocode-scraping/scripts/scrapingant-fetch.mjs --url https://docs.scrapingant.com/api-basics --mode markdown --crawl --same-domain --max-pages 2 --delay-ms 1000
node skills/octocode-scraping/scripts/scrapingant-fetch.mjs --url https://example.com --mode html --browser --wait-for '#content'
node skills/octocode-scraping/scripts/scrapingant-usage.mjs
```

## Supported script options
- `--session <id>`: write to `.octocode/tmp/scrape/<id>`.
- `--out <dir>`: base output dir; default `.octocode/tmp/scrape`.
- `--no-raw`: skip raw payload file.
- `--max-raw-bytes <n>` / `--max-text-bytes <n>`: bounded artifacts.
- `--extract-links`: write normalized link rows.
- `--crawl --max-pages <n>`: fetch discovered links into one corpus.
- `--sitemap`: seed crawl from `/sitemap.xml`.
- `--same-domain`: keep crawl within the target hostname.
- `--delay-ms <n>`: delay between crawl requests.
- `--proxy-type datacenter|residential`, `--proxy-country <code>`, `--block-resource <type>`: pass common ScrapingAnt controls.

## API params used by the script
- `browser=true` for JS rendering.
- `wait_for_selector=<css>` when browser rendering should wait for an element.
- Extra `--param key=value` is passed through after allowlisting by user intent.

## Common statuses
- `403`: wrong API token or credits exceeded; do not retry blindly.
- `404`: target URL unreachable; verify the URL.
- `405`: endpoint method mismatch.
- `422`: invalid parameter/value; fix sanitized params.
- `423`: anti-bot detection; reduce scope or ask before escalation.
- `500`: provider/server issue; retry later once.
