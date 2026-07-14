# Trend And Momentum Sources

Load when generic/undated web results cannot show momentum, crowdedness, published research, or whether a platform already shipped the idea. Skip for internal-only ideas or when `octocode-research` already settles repo/package activity. Recurring monitoring belongs in `octocode-news`.

| Need | Sources |
|---|---|
| Cross-domain momentum | Hacker News/HN RSS, Techmeme, Product Hunt, Indie Hackers, Google Trends, Exploding Topics; verify claims through primary sources |
| Papers/methods | arXiv recent/search, Google Scholar, Semantic Scholar, SSRN, PubMed, CORE/open-access publisher pages |
| Product landscape | G2, Capterra, AlternativeTo, launch history and reviews; use them to find competitors, not as proof alone |
| Repo/package momentum | GitHub Trending/Topics, Best of JS, Socket Trending, npm trends, Star History, OSS Insight; corroborate with exact repo/package data |
| AI-only signals | Hugging Face trending models/papers/spaces, Artificial Analysis, LMArena, Good AI List; pair with papers or primary releases |
| Security | CISA KEV, GitHub Advisories, OSV.dev |

For platform validation, go straight to the relevant vendor: OpenAI/Anthropic/Google/Hugging Face news; GitHub/Vercel/Node/Cloudflare/AWS changelogs; Chrome/TC39/Web Features for web standards.

## Rules

- A trend surface alone is `weak`; require an independent source or direct repo/package/paper data for `moderate`/`strong`.
- For scientific/technical claims, check papers and publisher/official sources before accepting blog or marketing summaries.
- Match sources to the domain: product → momentum + competitors; technical → papers; code-adjacent → repo/package; add AI/security sources only when relevant.
- Fetch at most 2-3 sources per section unless the user requests a landscape map.
- Record the dated signal—release/activity/citation/ranking/velocity—not merely presence.
- Apply `output.md` confidence rules and `tools.md` formal-source ladder to every citation.
