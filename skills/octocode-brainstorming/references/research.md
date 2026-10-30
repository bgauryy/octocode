Load when a gatherer or the parent starts searching or picks surfaces for trends, papers, products, releases, or advisories.

# Research surfaces

## Searching

- Combine `scripts/search.mjs` (configured providers, merged by canonical URL, multi-provider hits first) with host search and fetch. `--kind papers` for studies, `--kind news` for recent events, `--since` for a window.
- Results are leads: read the page before taking a claim. A page or syndicated story from several providers is one source.
- Vet sources like a [fact-checker](critical-thinking.md#search-like-a-fact-checker), including its stop rule.
- Keys: `TAVILY_API_KEY`, `EXA_API_KEY`, `SERPER_API_KEY`; load them using [shared configuration handling](configuration.md). A missing key skips that provider. A key that answers 401 or 403 on every query reports `status: "keyRejected"` with the exact `config set` fix; when every configured provider rejects its key the run fails with `keysRejected`: tell the user which keys to renew, and gather with the host's web search and fetch tools meanwhile.

## Surfaces

- Known stack: name language, framework, and versions in queries.
- Settle disagreements with the spec, paper, or official release.
- Projects and packages: inspect releases, maintainers, compatibility, issue resolution; neither inactivity nor a recent publish proves health alone.
- A Scholar hit is a lead; fetch the paper or publisher page.
- Blocked publisher: try arXiv or the author's page, an institutional repository, Europe PMC, or a DOI metadata abstract before an index mirror; label a mirror in the note.

## Providers

Use `--providers` only when one fits better.

| Provider | Strong at | `--kind` mapping |
|---|---|---|
| Tavily | general pages with readable extracts | `news`: news topic; `papers`: scholarly domains |
| Exa | semantic matches, similar pages, papers, publish dates | `news` and `papers`: its news and research-paper categories |
| Serper | Google results, news dates, Scholar with citation counts and PDF links | `news`: Google News; `papers`: Google Scholar |

Cross-provider rank shows relevance, not truth; `citedBy` shows attention, not quality.

## Momentum

A trend surface alone earns `low`; `medium` or `high` needs an independent source or direct repository, package, or paper data. Record the dated signal.

| Need | Sources |
|---|---|
| Cross-domain momentum | Hacker News RSS, Techmeme, Product Hunt, Indie Hackers, Google Trends, Exploding Topics |
| Papers and methods | arXiv, Google Scholar, Semantic Scholar, SSRN, PubMed, CORE, publisher page |
| Product landscape | G2, Capterra, AlternativeTo, launch history, reviews |
| Repository and package momentum | GitHub Trending, GitHub Topics, Best of JS, Socket Trending, npm trends, Star History, OSS Insight |
| AI signals (only when needed) | Hugging Face trending; Artificial Analysis; LMArena; Good AI List |
| Security (only when needed) | CISA KEV, GitHub Advisories, OSV.dev |
| A platform already shipped it | That vendor's changelog or news page |

Listings are leads; corroborate on the primary source (repository, package, paper, release). Thin evidence: change the question or source, use `octocode-research` for code gaps, report what stays uncovered.

Next: [decide](../SKILL.md#4-decide).
