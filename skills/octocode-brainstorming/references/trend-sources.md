# Trend Sources — momentum, articles & research signal

Load during RESEARCH/CROSS-POLLINATE when Tavily/Serper return generic or undated results and the idea
needs a momentum/crowdedness signal, a published-research check, or confirmation a platform already
shipped it. Generic across domains — not AI/devtools-only. Curated from `octocode-news`'s source
catalog (`_skills/octocode-news/references`, file `sources.md`; full RSS catalog) plus general
research/idea surfaces, scoped down for one-off validation — not a monitoring pipeline. No RSS fetcher
here: fetch the URL directly with the web reader and cite it like any web result (`output.md` rules
apply; `tools.md`'s Formal web ladder governs citation grading).

Skip when `ghSearchRepos`/`npmSearch` already settles the question, or the idea is purely internal. For
recurring space-watching, hand off to `octocode-news` instead of repeating checks here.

## Cross-domain momentum & ideas (any idea)

Hacker News (`news.ycombinator.com`, RSS `hnrss.org/frontpage`, keyword feed `hnrss.org/newest?q=<term>`) · Techmeme (RSS `techmeme.com/feed.xml`) · Product Hunt (`producthunt.com`) · Indie Hackers (`indiehackers.com`) · Google Trends (`trends.google.com/trends`) · Exploding Topics (`explodingtopics.com`) · r/SideProject, r/Entrepreneur (Reddit — verify via primary sources)

## Articles & papers — the generic evidence surface, check first for any method/technical claim

**arXiv is highly recommended and covers far more than AI**: list by subject at `arxiv.org/list/<subject>/recent`
(`cs.AI`/`cs.CL`/`cs.LG`/`cs.CV`/`cs.RO` for AI/robotics; `stat.ML`/`math.OC` for methods; `q-bio.*`/`q-fin.*`/`econ.*`/`eess.SY` for biology/finance/economics/systems); search `arxiv.org/search/?searchtype=all&query=<terms>`, sort by submission date for recency. Faster and more current than a general web search — it predates most blog coverage of a technique.

Cross-check with: Google Scholar (`scholar.google.com` — use "cited by" for real-world impact) · Semantic Scholar (`semanticscholar.org`) · SSRN (`ssrn.com`, economics/business/law) · PubMed (`pubmed.ncbi.nlm.nih.gov`, life sciences/health) · CORE (`core.ac.uk`, open-access copies when a paper is paywalled).

## Competitive & market landscape (product/business ideas)

G2 (`g2.com`) · Capterra (`capterra.com`) · AlternativeTo (`alternativeto.net`) · Product Hunt (above, for launch history) — read reviews/comparisons for "who else does this, and how well," not just "does it exist."

## Repo & package momentum (code-adjacent ideas)

GitHub Trending (`github.com/trending?since=daily`; RSS `mshibanami.github.io/GitHubTrendingRSS/daily/{lang}.xml`, also `weekly/`/`monthly/`) · GitHub Topics (`github.com/topics/<topic>`) · Best of JS (`bestofjs.org`) · Socket Trending (`socket.dev/npm/category/trending`) · npm trends (`npmtrends.com`) · Star History (`star-history.com`) · OSS Insight (`ossinsight.io`)

Corroborate with `ghSearchRepos`/`npmSearch` — these surfaces rank/compare, they don't replace tool evidence.

## AI trend surfaces (AI ideas only)

HF Trending Models/Papers/Spaces (`huggingface.co/models?sort=trending`, `/papers/trending`, `/spaces?sort=trending`) · Artificial Analysis (`artificialanalysis.ai`) · LMArena (`arena.ai`) · Good AI List API — JSON, no RSS (`goodailist.com/api/repos?sort=stars&order=desc`). For arXiv AI listings, see Articles & Papers above.

## Platform validation — "did they already ship this"

Go straight to the one vendor that matters; don't browse speculatively.

- **AI**: OpenAI News (`openai.com/news`, RSS `.../news/rss.xml`) · Anthropic News (`anthropic.com/news`) · Google AI Blog (RSS `blog.google/innovation-and-ai/technology/ai/rss/`) · HF Blog (RSS `huggingface.co/blog/feed.xml`)
- **Devtools**: Vercel Changelog (RSS `vercel.com/atom`) · GitHub Changelog (RSS `github.blog/changelog/feed/`) · Node.js Blog (RSS `nodejs.org/en/feed/blog.xml`) · Cloudflare Blog (RSS `blog.cloudflare.com/rss/`) · AWS What's New (RSS `aws.amazon.com/about-aws/whats-new/recent/feed/`)
- **Web**: Chrome Dev Blog (RSS `developer.chrome.com/blog/feed.xml`) · TC39 Proposals (`github.com/tc39/proposals`) · web-features/Baseline (`github.com/web-platform-dx/web-features`)

## Security signal (security/vuln-tooling ideas only)

CISA KEV (`cisa.gov/known-exploited-vulnerabilities-catalog`) · GitHub Advisories (`github.com/advisories`) · OSV.dev

## Deeper landscape maps

`plenaryapp/awesome-rss-feeds` · `vishalshar/awesome_ML_AI_RSS_feed` (list itself dormant since 2021, entries still valid) · `alvinreal/awesome-opensource-ai` · `kilimchoi/engineering-blogs` · `duanyytop/agents-radar` — read via `ghGetFileContent`/`WebFetch`; don't duplicate their contents into this file.

## Rules

- A trend surface alone rates `weak` (per `output.md`); needs a second independent source or direct repo/package/paper data for `moderate`/`strong`.
- For any method/technical/scientific claim, check arXiv/Scholar/Semantic Scholar before treating a blog or marketing claim as validated — published research beats a generic web search for recency and rigor.
- Match the section to the idea's domain: business/product idea → Cross-domain momentum + Competitive landscape; technical/scientific claim → Articles & papers; code-adjacent → Repo & package momentum (+ AI trend surfaces if AI-specific).
- Cap 2-3 fetches per section per brainstorm unless the task is explicitly a landscape map.
