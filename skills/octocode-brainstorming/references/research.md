Load when checking search keys, choosing a vendor or `--kind`, wording queries, or picking surfaces for trends, papers, products, releases or advisories.

# Research surfaces

## Keys

[search.mjs](../scripts/search.mjs) and [verify-claims.mjs](../scripts/verify-claims.mjs) read keys only through `getKey` in [keys.mjs](../scripts/keys.mjs). The first place that sets a key wins, in the [API key order](configuration.md#where-a-value-can-live): process environment, then the home `.env` (`OCTOCODE_HOME`, default `~/.octocode`), then the workspace `.octocode/.env` (the current directory).

```bash
node <skill-folder>/scripts/keys.mjs get TAVILY_API_KEY EXA_API_KEY SERPER_API_KEY DEEPSEEK_API_KEY   # set/missing, source; exit 1 if one is missing
node <skill-folder>/scripts/keys.mjs list                                                             # every declared Octocode key
node <skill-folder>/scripts/keys.mjs set TAVILY_API_KEY --stdin [--workspace]                         # paste or pipe; saves to the home .env
```

- No command prints a value; `files` names the home and workspace files read. A missing key carries its `fix`.
- Run from the workspace root. From a subfolder, `<workspace>` is that subfolder, so a key only in the repository's `.octocode/.env` is missed; the `workspace` path under `files` in the output shows which file was read.
- A workspace `.env` only fills a key you have not set globally. `TAVILY_BASE_URL`, `EXA_BASE_URL`, `SERPER_BASE_URL` and `DEEPSEEK_BASE_URL` come only from the process environment or the home `.env`, so a repository cannot send your key to its own host.
- Keep keys out of briefs, ledgers, reports and logs. Other settings: [shared configuration](configuration.md).

| Search output | Meaning | Action |
|---|---|---|
| `error.code: "missingKey"`, exit 1 | No key for the selected providers; the message names the files read | Save a key with `keys.mjs set`, or rerun from the folder whose `.octocode/.env` holds it; meanwhile use the host's web search and fetch |
| provider `status: "skipped"` | That key is not set, or the vendor has no search of that `--kind` (`reason` says which); the others ran | None, unless that vendor's surface is needed |
| provider `status: "keyRejected"` | HTTP 401 or 403 on every query: the key is wrong or revoked | Tell the user to renew it with the row's `fix` |
| provider `status: "quotaExceeded"` | Exa or DeepSeek 402, Tavily 432 or 433: out of credits or over a plan limit | Tell the user to add credits at the row's `fix` link; use the other vendors meanwhile |
| provider `status: "rateLimited"` | HTTP 429 on every query; `retryAfterSeconds` when the vendor sent one | Wait that long, then retry with fewer queries |
| `error.code: "keysRejected"` / `"providersUnavailable"` | Every configured vendor rejected its key / failed for one of the reasons above | Apply each provider's fix; gather with host tools meanwhile |
| provider `status: "error"` with `errors[]` | Timeout or server error | Retry once with fewer queries; do not repeat an unchanged failing call |
| `error.code: "invalidArguments"`, exit 2 | A bad flag, or `--providers` names only vendors without that `--kind` (`--providers deepseek --kind news`) | Fix the flag; the message lists the vendors that serve the kind |
| `error.code: "noResults"` | Every provider answered with nothing | Reword, change `--kind` or drop filters |

## Vendors

A search runs every vendor that has a key, one request per query each; `--providers` restricts it when one surface fits better.

| Vendor | Strong at | `--kind news` | `--kind papers` | Results carry |
|---|---|---|---|---|
| Tavily | General pages with readable extracts | News topic | Scholarly domains (arXiv, OpenReview, ACL, ACM, IEEE, PubMed, …) unless `--include-domain` names your own | `published` (Tavily's estimate, RFC 1123); `credits` per provider (1 per query) |
| Exa | Semantic matches, papers, publish dates | News category | `publication` category | `published` (ISO); `costDollars` per provider |
| Serper | Google results, Google News, Scholar with `citedBy` and `pdfUrl` | Google News | Google Scholar; `--since` filters only by year | `published` as Google shows it (`Aug 22, 2025`, `2 weeks ago`) or a year; `credits` (1 per 10 results); `related` |
| DeepSeek | A second general web index with model-ready summaries | Skipped | Skipped | `published` (RFC 3339) when known; no cost field (billed per request that returns results) |

- Serper returns at most 10 results per request, so `--limit` above 10 reads a second page (one more credit).
- DeepSeek takes filters inside the query: `--include-domain` becomes `site:` clauses joined with `OR`, `--exclude-domain` becomes `-site:`, and `--since` becomes `after:`. It refuses a query over 128 characters, operators included, with HTTP 400, so keep DeepSeek queries short and name few domains.
- `related` lists Google's related searches and "People also ask" questions for a web search; Google omits them when `--since` is set. They are leads for options and disconfirming queries, not evidence.
- Results merge by canonical URL; a page several vendors or queries return ranks first. A page from three vendors is still one source. Rank shows relevance, not truth; `citedBy` shows attention, not quality.
- `published` is each vendor's own format and estimate. Confirm the date on the page: `verify-claims.mjs` reports the page's `published` and `modified`.
- A snippet is a lead, never a quote. Open the page before taking a claim from it.

## Write queries

- Name the language, framework, product and version; add `--since YYYY-MM-DD` when recency matters.
- Search each deciding claim both ways. Support: the claim in its own words. Against: `<option> problems`, `<option> limitations`, `<option> postmortem`, `<option> vs <rival>`, `migrating away from <option>`, `<claim> criticism`, `<claim> replication`.
- Batch distinct queries with repeated `--query`; vary wording to separate hypotheses, not to reach a count.
- Stop a line when varied searches add no deciding evidence; "no evidence found" in the searched scope is a valid result. Name the gap instead of padding the source list.

## Surfaces

- Settle disagreements with the spec, paper or official release.
- Projects and packages: inspect releases, maintainers, compatibility and issue resolution; neither inactivity nor a recent publish proves health alone.
- A Scholar hit is a lead; fetch the paper or publisher page. Blocked publisher: try arXiv or the author's page, an institutional repository, Europe PMC or a DOI metadata abstract before an index mirror; label a mirror.

A trend listing is weak evidence for effectiveness or adoption. Corroborate on the primary source (repository, package, paper, release) and use the [confidence rubric](critical-thinking.md#calibrated-confidence).

| Need | Sources |
|---|---|
| Cross-domain momentum | Hacker News RSS, Techmeme, Product Hunt, Indie Hackers, Google Trends, Exploding Topics |
| Papers and methods | arXiv, Google Scholar, Semantic Scholar, SSRN, PubMed, CORE, publisher page |
| Product landscape | G2, Capterra, AlternativeTo, launch history, reviews |
| Repository and package momentum | GitHub Trending, GitHub Topics, Best of JS, Socket Trending, npm trends, Star History, OSS Insight |
| AI signals (only when needed) | Hugging Face trending; Artificial Analysis; LMArena; Good AI List |
| Security (only when needed) | CISA KEV, GitHub Advisories, OSV.dev |
| A platform already shipped it | That vendor's changelog or news page |

Thin evidence: change the question or source, use `octocode-research` for code gaps, and report what stays uncovered.
