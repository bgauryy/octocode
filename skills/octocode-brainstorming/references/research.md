Load when choosing a search vendor or `--kind`, wording queries, or picking surfaces for trends, papers, products, releases or advisories. Keys and search errors: [search keys](research-keys.md); GitHub research and combining it with the web: [GitHub](research-github.md).

# Research surfaces

## Vendors

A search runs every vendor that has a key, one request per query each; `--providers` restricts it when one surface fits better.

| Vendor | Strong at | `--kind news` | `--kind papers` | Results carry |
|---|---|---|---|---|
| Tavily | General pages with readable extracts | News topic | Scholarly domains (arXiv, OpenReview, ACL, ACM, IEEE, PubMed, …) unless `--include-domain` names your own | `published` (Tavily's estimate, RFC 1123); `credits` per provider (1 per query) |
| Exa | Semantic matches, papers, publish dates | News category | `publication` category | `published` (ISO); `costDollars` per provider |
| Serper | Google results, Google News, Scholar with `citedBy` and `pdfUrl` | Google News | Google Scholar; `--since` filters only by year | `published` as Google shows it (`Aug 22, 2025`, `2 weeks ago`) or a year; `credits` (1 per 10 results); `related` |
| DeepSeek | A second general web index with model-ready summaries | Skipped | Skipped | `published` (RFC 3339) when known, written as midnight Beijing time in UTC (`T16:00:00+00:00`), so its date reads one day early; no cost field (billed per request that returns results) |

- Serper returns at most 10 results per request, so `--limit` above 10 reads a second page (one more credit).
- DeepSeek takes filters inside the query: `--include-domain` becomes `site:` clauses joined with `OR`, `--exclude-domain` becomes `-site:`, and `--since` becomes `after:`. It refuses a query over 128 characters, operators included, with HTTP 400, so keep DeepSeek queries short and name few domains.
- `related` lists Google's related searches and "People also ask" questions for a web search; Google omits them when `--since` is set. They are leads for options and disconfirming queries, not evidence.
- Results merge by canonical URL; a page several vendors or queries return ranks first. A page from three vendors is still one source. Rank shows relevance, not truth; `citedBy` shows attention, not quality.
- `published` is each vendor's own format and estimate. Confirm the date on the page: `verify-claims.mjs` reports the page's `published` and `modified`.
- A snippet is a lead until the page is [opened](verification.md#validated-sources) and the quote copied from it.

## Write queries

- Name the language, framework, product and version; add `--since YYYY-MM-DD` when recency matters.
- Search each deciding claim both ways. Support: the claim in its own words. Against: `<option> problems`, `<option> limitations`, `<option> postmortem`, `<option> vs <rival>`, `migrating away from <option>`, `<claim> criticism`, `<claim> replication`.
- Batch distinct queries with repeated `--query`; vary wording to separate hypotheses, not to reach a count.
- Stop a line when varied searches add no deciding evidence; "no evidence found" in the searched scope is a valid result. Name the gap instead of padding the source list.

## Surfaces

- Settle disagreements with the spec, paper or official release.
- Projects and packages: inspect releases, maintainers, compatibility and issue resolution; neither inactivity nor a recent publish proves health alone.
- A Scholar hit is a lead; fetch the paper or publisher page. Blocked publisher: try arXiv or the author's page, an institutional repository, Europe PMC or a DOI metadata abstract before an index mirror; label a mirror.

A trend listing is weak evidence for effectiveness or adoption. Corroborate on the primary source (repository, package, paper, release) and use the [confidence rubric](proof-bar.md#calibrated-confidence).

| Need | Sources |
|---|---|
| Cross-domain momentum | Hacker News RSS, Techmeme, Product Hunt, Indie Hackers, Google Trends, Exploding Topics |
| Papers and methods | arXiv, Google Scholar, Semantic Scholar, SSRN, PubMed, CORE, publisher page |
| Product landscape | G2, Capterra, AlternativeTo, launch history, reviews |
| Repository and package momentum | GitHub Trending, GitHub Topics, Best of JS, Socket Trending, npm trends, Star History, OSS Insight |
| AI signals (only when needed) | Hugging Face trending; Artificial Analysis; Arena (arena.ai; named LMArena until January 2026); Good AI List |
| Security (only when needed) | CISA KEV, GitHub Advisories, OSV.dev |
| A platform already shipped it | That vendor's changelog or news page |

Thin evidence: change the question or source, use `octocode-research` for code gaps, and report what stays uncovered.
