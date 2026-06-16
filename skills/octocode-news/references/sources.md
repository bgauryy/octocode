# Source Catalog

> Last verified: Apr 2026

## Execution Protocol

1. **User overrides** → apply first
2. **Cross-domain discovery** → scan early, expand continuously
3. **Domain Primary** → check in order
4. **Domain Secondary** → corroborate
5. **Validation** → prove shipped code/releases
6. **Optional** → when primary exhausted or new signals emerge

**Evidence:** 1 primary + 1 validation per item. Cite explicitly in report.

**Web-content rule:** For every kept website/RSS item, open the canonical article URL and read full page content before summarizing. RSS snippets are discovery-only.

**Expansion:** Catalog is baseline, not ceiling. Use any relevant source (GitHub, vendor changelogs, trending).

## Source Tiers

**P1 (must-check at all depths):** Highest-signal sources per domain. Check these first, even at `brief` depth.
**P2 (deep and comprehensive only):** Secondary and supplemental. Skip at `brief` depth unless P1 coverage is thin.

| Domain | P1 Sources |
|--------|------------|
| AI | OpenAI News, Anthropic News, Google AI Blog, Google DeepMind, Hugging Face Blog, Meta AI Blog, Mistral, DeepSeek, Simon Willison, The Decoder, HF Trending Papers |
| Devtools | Vercel Changelog, Node.js Blog, Bun Blog, Deno Blog, TypeScript Blog, GitHub Changelog, Cursor Changelog, Cloudflare Blog, AWS What's New |
| Web | Chrome Dev Blog, web.dev, WebKit Blog, MDN Blog, JavaScript Weekly, TC39 Proposals |
| Security | CISA KEV, GitHub Advisories, The Hacker News, BleepingComputer, Krebs on Security, Schneier on Security |
| Repos | GitHub Trending, GitHub Explore, Best of JS, HF Trending Models, Good AI List API |
| Cross | Hacker News, Techmeme, TechCrunch, The Verge, Ars Technica |

---

## A — AI

Model labs, API/platform changes, research, and open-model momentum.

### Official Sources

| Source               | URL                                                   | RSS                                                  |
| -------------------- | ----------------------------------------------------- | ---------------------------------------------------- |
| OpenAI News          | https://openai.com/news                               | `openai.com/news/rss.xml`                            |
| OpenAI Changelog     | https://platform.openai.com/docs/changelog            | —                                                    |
| Anthropic News       | https://www.anthropic.com/news                        | —                                                    |
| Google AI Blog       | https://blog.google/innovation-and-ai/technology/ai/  | `blog.google/innovation-and-ai/technology/ai/rss/`   |
| Google DeepMind Blog | https://deepmind.google/blog/                         | `deepmind.google/blog/rss.xml`                       |
| Gemini API Changelog | https://ai.google.dev/gemini-api/docs/changelog       | —                                                    |
| Hugging Face Blog    | https://huggingface.co/blog                           | `huggingface.co/blog/feed.xml`                       |
| Meta AI Blog         | https://ai.meta.com/blog/                             | —                                                    |
| Mistral News         | https://mistral.ai/news/                              | —                                                    |
| DeepSeek News        | https://github.com/deepseek-ai                        | —                                                    |
| xAI Blog             | https://x.ai/blog                                     | — (challenge-protected)                              |
| Perplexity Hub       | https://www.perplexity.ai/hub/                        | — (challenge-protected)                              |
| ElevenLabs Blog      | https://elevenlabs.io/blog                            | —                                                    |
| Runway Research      | https://runwayml.com/research/                        | —                                                    |
| Allen AI Research    | https://allenai.org/research                          | —                                                    |
| Cerebras Blog        | https://www.cerebras.ai/blog                          | —                                                    |
| Qwen Blog            | https://qwenlm.github.io/blog/                        | —                                                    |
| Stability AI News    | https://stability.ai/news-updates                     | —                                                    |
| NVIDIA AI Blog       | https://blogs.nvidia.com/blog/category/deep-learning/ | `blogs.nvidia.com/blog/category/deep-learning/feed/` |
| Microsoft Research   | https://www.microsoft.com/en-us/research/             | `microsoft.com/en-us/research/feed/`                 |
| EleutherAI Blog      | https://blog.eleuther.ai/                             | `blog.eleuther.ai/index.xml`                         |
| Apple ML Research    | https://machinelearning.apple.com/                    | `machinelearning.apple.com/rss.xml`                  |
| Amazon Science       | https://www.amazon.science/                           | `amazon.science/index.rss`                           |
| Google Research Blog | https://research.google/blog/                         | — (RSS broken since Oct 2025, use website)           |

### News

| Source                       | URL                                                                       | RSS                                                               |
| ---------------------------- | ------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| The Decoder                  | https://the-decoder.com/                                                  | `the-decoder.com/feed/`                                           |
| IEEE Spectrum AI             | https://spectrum.ieee.org/topic/artificial-intelligence/                  | —                                                                 |
| Artificial Intelligence News | https://www.artificialintelligence-news.com/                              | `artificialintelligence-news.com/feed/`                           |
| Bloomberg AI                 | https://www.bloomberg.com/ai                                              | — (challenge-protected)                                           |
| The Information              | https://www.theinformation.com/                                           | — (paywalled)                                                     |
| BBC AI                       | —                                                                         | `feeds.bbci.co.uk/news/topics/ce1qrvleleqt/rss.xml`               |
| The Guardian AI              | https://www.theguardian.com/technology/artificialintelligenceai           | `theguardian.com/technology/artificialintelligenceai/rss`         |
| WIRED AI                     | https://www.wired.com/tag/artificial-intelligence/                        | —                                                                 |
| WSJ AI                       | https://www.wsj.com/tech/ai                                               | — (paywalled)                                                     |
| CNN AI                       | https://edition.cnn.com/business/tech/ai-news-artificial-intelligence-updates | —                                                             |
| Economist AI                 | https://www.economist.com/topics/artificial-intelligence                  | — (challenge-protected)                                           |
| FT AI                        | —                                                                         | `ft.com/artificial-intelligence?format=rss`                       |
| MarkTechPost                 | https://www.marktechpost.com/                                             | — (feed blocked, use website)                                     |
| 404 Media                    | https://www.404media.co/                                                  | `404media.co/rss`                                                 |
| ScienceDaily AI              | https://www.sciencedaily.com/news/computers_math/artificial_intelligence/ | `sciencedaily.com/rss/computers_math/artificial_intelligence.xml` |
| AI Business                  | https://aibusiness.com/                                                   | `aibusiness.com/rss.xml`                                          |
### Newsletters & Roundups

| Source                  | URL                                    | RSS                                  |
| ----------------------- | -------------------------------------- | ------------------------------------ |
| Superhuman AI           | https://www.superhuman.ai/             | — (challenge-protected)              |
| The Rundown AI          | https://www.therundown.ai/             | — (challenge-protected)              |
| AINews (smol.ai)        | https://news.smol.ai                   | `news.smol.ai/rss.xml`               |
| AI Weekly               | —                                      | `ai-weekly.ai/feed/`                 |
| AI Weekly (aiweekly.co) | —                                      | `aiweekly.co/feed`                   |
| The Batch               | https://www.deeplearning.ai/the-batch/ | —                                    |
| TLDR AI                 | https://tldr.tech/ai                   | —                                    |
| Last Week in AI         | https://lastweekin.ai/                 | `lastweekin.ai/feed`                 |
| SemiAnalysis            | https://www.semianalysis.com/          | `semianalysis.com/feed` (dormant since Sep 2025, check web) |
| Normal Tech (AI Snake Oil) | https://www.normaltech.ai/          | `normaltech.ai/feed`                 |
| The Gradient            | https://thegradient.pub/               | `thegradient.pub/rss/`               |
| KDnuggets               | https://www.kdnuggets.com/             | `kdnuggets.com/feed`                 |
| AIModels.fyi            | https://aimodels.substack.com/         | `aimodels.substack.com/feed`         |
| Chain of Thought        | https://every.to/chain-of-thought/     | `every.to/chain-of-thought/feed.xml` |
| Ben's Bites             | https://www.bensbites.com/             | `bensbites.com/feed`                 |
| Import AI               | https://importai.substack.com/         | `importai.substack.com/feed`         |
| The Sequence            | https://thesequence.substack.com/      | `thesequence.substack.com/feed`      |
| Latent Space            | https://www.latent.space/              | `latent.space/feed`                  |
| Interconnects           | https://www.interconnects.ai/          | `www.interconnects.ai/feed`          |
| Unwind AI               | https://www.theunwindai.com/           | `rss.beehiiv.com/feeds/fMHDv0Uk41.xml` |

### Research & Papers

| Source             | URL                                                 | RSS                                               |
| ------------------ | --------------------------------------------------- | ------------------------------------------------- |
| arXiv cs.AI        | https://arxiv.org/list/cs.AI/recent                 | — (web-only; RSS feed is undated, use listing page) |
| arXiv cs.CL        | https://arxiv.org/list/cs.CL/recent                 | — (web-only; RSS feed is undated, use listing page) |
| arXiv cs.LG        | https://arxiv.org/list/cs.LG/recent                 | — (web-only; RSS feed is undated, use listing page) |
| arXiv cs.CV        | https://arxiv.org/list/cs.CV/recent                 | — (web-only; RSS feed is undated, use listing page) |
| JMLR               | https://www.jmlr.org/                               | `jmlr.org/jmlr.xml`                               |
| MIT News AI        | https://news.mit.edu/topic/artificial-intelligence2 | `news.mit.edu/rss/topic/artificial-intelligence2` |
| Berkeley BAIR Blog | https://bair.berkeley.edu/blog/                     | `bair.berkeley.edu/blog/feed.xml`                 |
| ML@CMU Blog        | https://blog.ml.cmu.edu/                            | `blog.ml.cmu.edu/feed`                            |

### Personal Blogs

| Source                   | URL                                    | RSS                                    |
| ------------------------ | -------------------------------------- | -------------------------------------- |
| Simon Willison           | https://simonwillison.net/             | `simonwillison.net/atom/everything/`   |
| Chip Huyen               | https://huyenchip.com/                 | — (dormant since Jan 2025)             |
| Lil'Log (Lilian Weng)    | https://lilianweng.github.io/          | — (dormant since May 2025)             |
| Eugene Yan               | https://eugeneyan.com/                 | — (dormant since Dec 2025, check web)  |
| Sebastian Raschka        | https://magazine.sebastianraschka.com/ | `magazine.sebastianraschka.com/feed`   |
| One Useful Thing (Ethan Mollick) | https://www.oneusefulthing.org/ | `oneusefulthing.substack.com/feed`     |
| Marcus on AI (Gary Marcus)       | https://garymarcus.substack.com/ | `garymarcus.substack.com/feed`        |
| The Algorithmic Bridge           | https://thealgorithmicbridge.substack.com/ | `thealgorithmicbridge.substack.com/feed` |

### Trend Surfaces

| Source                     | URL                                                |
| -------------------------- | -------------------------------------------------- |
| HF Trending Models         | https://huggingface.co/models?sort=trending        |
| HF Trending Datasets       | https://huggingface.co/datasets?sort=trending      |
| HF Trending Papers         | https://huggingface.co/papers/trending             |
| HF Trending Spaces         | https://huggingface.co/spaces?sort=trending        |
| GitHub Trending (Daily)    | https://github.com/trending?since=daily            |
| GitHub Trending Python     | https://github.com/trending/python?since=daily     |
| GitHub Trending TypeScript | https://github.com/trending/typescript?since=daily |
| GitHub AI Topic            | https://github.com/topics/artificial-intelligence  |
| Artificial Analysis        | https://artificialanalysis.ai/                     |
| LMArena                    | https://arena.ai/                                  |

### Repos (Validation)

`google-gemini/gemini-cli`, `openai/codex`, `openclaw/openclaw`

`huggingface/transformers`, `huggingface/datasets`, `huggingface/text-generation-inference`, `huggingface/text-embeddings-inference`, `vllm-project/vllm`, `sgl-project/sglang`, `ollama/ollama`, `ggml-org/llama.cpp`, `NVIDIA/TensorRT-LLM`, `BerriAI/litellm`

`openai/openai-python`, `openai/openai-node`, `openai/tiktoken`, `anthropics/anthropic-sdk-python`, `anthropics/anthropic-sdk-typescript`, `googleapis/python-genai`, `mistralai/client-python`, `cohere-ai/cohere-python`, `deepseek-ai/DeepSeek-V3`, `QwenLM/Qwen`, `QwenLM/Qwen-Agent`, `QwenLM/Qwen-Image`

`modelcontextprotocol/servers`, `microsoft/autogen`, `crewAIInc/crewAI`, `run-llama/llama_index`, `langchain-ai/langchain`, `open-webui/open-webui`, `Comfy-Org/ComfyUI`, `AUTOMATIC1111/stable-diffusion-webui`

### Community

r/LocalLLaMA, r/MachineLearning, r/deeplearning, r/artificial, Hacker News AI threads — verify via primary sources.

Reddit RSS pattern: `https://www.reddit.com/r/{subreddit}/.rss` (blocked by curl, works in readers).

### Optional Aggregators

AI Engineer, The Neuron, Turing Post, Techpresso

---

## B — Devtools

Runtimes, cloud platforms, build tools, package managers, and infra changelogs.

### Websites

| Source              | URL                                        | RSS                                               |
| ------------------- | ------------------------------------------ | ------------------------------------------------- |
| Vercel Changelog    | https://vercel.com/changelog               | `vercel.com/atom`                                 |
| Cloudflare Blog     | https://blog.cloudflare.com                | `blog.cloudflare.com/rss/`                        |
| Node.js Blog        | https://nodejs.org/en/blog                 | `nodejs.org/en/feed/blog.xml`                     |
| Bun Blog            | https://bun.sh/blog                        | —                                                 |
| Deno Blog           | https://deno.com/blog                      | `deno.com/feed`                                   |
| TypeScript Blog     | https://devblogs.microsoft.com/typescript/ | `devblogs.microsoft.com/typescript/feed/`         |
| Kubernetes Releases | https://kubernetes.io/releases/            | `kubernetes.io/feed.xml`                          |
| Docker Blog         | https://www.docker.com/blog/               | `docker.com/blog/feed/`                           |
| GitHub Changelog    | https://github.blog/changelog/             | `github.blog/changelog/feed/`                     |
| Cursor Changelog    | https://cursor.com/changelog               | —                                                 |
| Rust Blog           | https://blog.rust-lang.org/                | `blog.rust-lang.org/feed.xml`                     |
| Go Blog             | https://go.dev/blog                        | `go.dev/blog/feed.atom`                           |
| AWS What's New      | https://aws.amazon.com/new/                | `aws.amazon.com/about-aws/whats-new/recent/feed/` |
| Google Cloud Blog   | https://cloud.google.com/blog              | —                                                 |
| InfoQ               | https://www.infoq.com                      | `feed.infoq.com/`                                 |
| The New Stack       | https://thenewstack.io                     | `thenewstack.io/feed/`                            |
| TLDR DevOps         | https://tldr.tech/devops                   | —                                                 |
| Node Weekly         | https://nodeweekly.com                     | `nodeweekly.com/rss/`                             |
| Golang Weekly       | https://golangweekly.com                   | `golangweekly.com/rss/`                           |
| Changelog News      | https://changelog.com/news                 | `changelog.com/news/feed`                         |

### Repos (Validation)

`oven-sh/bun`, `denoland/deno`, `nodejs/node`, `microsoft/TypeScript`, `pnpm/pnpm`, `biomejs/biome`, `vercel/turborepo`, `evanw/esbuild`, `docker/cli`, `kubernetes/kubernetes`, `rust-lang/rust`

---

## C — Web Platform

Browser APIs, JavaScript language proposals, CSS/platform features, and cross-browser shipping status.

### Websites

| Source                 | URL                                                       | RSS                                  |
| ---------------------- | --------------------------------------------------------- | ------------------------------------ |
| Chrome Dev Blog        | https://developer.chrome.com/blog                         | `developer.chrome.com/blog/feed.xml` |
| web.dev                | https://web.dev                                           | `web.dev/feed.xml`                   |
| WebKit Blog            | https://webkit.org/blog/                                  | `webkit.org/feed/`                   |
| MDN Blog               | https://developer.mozilla.org/en-US/blog/                 | —                                    |
| Mozilla Hacks          | https://hacks.mozilla.org/                                | `hacks.mozilla.org/feed/`            |
| TC39 Proposals         | https://github.com/tc39/proposals                         | —                                    |
| Chrome Platform Status | https://chromestatus.com/features                         | —                                    |
| Baseline               | https://github.com/web-platform-dx/web-features           | —                                    |
| JavaScript Weekly      | https://javascriptweekly.com                              | `javascriptweekly.com/rss/`          |
| Frontend Focus         | https://frontendfoc.us                                    | `frontendfoc.us/rss/`                |
| Bytes.dev              | https://bytes.dev                                         | —                                    |
| CSS Weekly             | https://css-weekly.com                                    | —                                    |
| Smashing Magazine      | https://www.smashingmagazine.com/the-smashing-newsletter/ | —                                    |
| State of JS            | https://stateofjs.com                                     | —                                    |

### Repos (Validation)

`tc39/proposals`, `whatwg/html`, `w3c/csswg-drafts`, `mdn/content`, `mdn/browser-compat-data`, `web-platform-dx/web-features`

### Framework-specific (when trending)

React Blog, Vue Blog, Svelte Blog, Angular Blog, This Week In React

`facebook/react`, `vercel/next.js`, `vitejs/vite`, `vuejs/core`, `sveltejs/svelte`, `nuxt/nuxt`, `angular/angular`, `tailwindlabs/tailwindcss`, `honojs/hono`

---

## D — Security

Vulnerability databases, incident reporting, exploit activity, and supply-chain monitoring.

### Websites

| Source              | URL                                                          | RSS                                                  |
| ------------------- | ------------------------------------------------------------ | ---------------------------------------------------- |
| CISA KEV            | https://www.cisa.gov/known-exploited-vulnerabilities-catalog | `cisa.gov/cybersecurity-advisories/all.xml`          |
| GitHub Advisories   | https://github.com/advisories                                | —                                                    |
| NVD                 | https://nvd.nist.gov/                                        | —                                                    |
| OSV.dev             | https://osv.dev/                                             | —                                                    |
| OpenCVE             | https://opencve.io/                                          | —                                                    |
| Google Project Zero | https://googleprojectzero.blogspot.com/                      | — (feed blocked, use website)                        |
| Microsoft Security  | https://msrc.microsoft.com/blog                              | —                                                    |
| Snyk Advisories     | https://security.snyk.io/                                    | —                                                    |
| Cloudflare Security | https://blog.cloudflare.com/tag/security/                    | —                                                    |
| tl;dr sec           | https://tldrsec.com                                          | —                                                    |
| Schneier on Security | —                                                          | `schneier.com/feed/atom/`                            |
| Krebs on Security   | https://krebsonsecurity.com                                  | `krebsonsecurity.com/feed/`                          |
| Risky Business      | https://risky.biz/                                           | —                                                    |
| BleepingComputer    | https://www.bleepingcomputer.com                             | `bleepingcomputer.com/feed/`                         |
| The Hacker News     | https://thehackernews.com                                    | `feeds.feedburner.com/TheHackersNews`                |
| TLDR InfoSec        | https://tldr.tech/infosec                                    | —                                                    |
| Wiz Research        | https://www.wiz.io/blog                                      | —                                                    |
| CrowdStrike Blog    | https://www.crowdstrike.com/en-us/blog/                      | `crowdstrike.com/en-us/blog/feed/`                   |
| SentinelOne Labs    | https://www.sentinelone.com/labs/                             | `sentinelone.com/labs/feed/`                         |
| Elastic Security    | https://www.elastic.co/security-labs                          | `elastic.co/security-labs/rss/feed.xml`              |
| Mandiant (Google)   | https://cloud.google.com/blog/topics/threat-intelligence      | `mandiant.com/resources/blog/rss.xml`                |
| PortSwigger Research | https://portswigger.net/research                             | `portswigger.net/research/rss`                       |

### Validation

- Affected project GHSA pages (GitHub advisories)
- Patched release notes and incident postmortems
- Use `ghSearchCode` / `ghViewRepoStructure` to confirm fix landed

---

## E — Repos & Releases

Trending repositories, star velocity, changelogs, and release notes across categories.

### Websites

| Source          | URL                                      |
| --------------- | ---------------------------------------- |
| GitHub Explore  | https://github.com/explore               |
| GitHub Blog     | https://github.blog                      |
| Best of JS      | https://bestofjs.org                     |
| Socket Trending | https://socket.dev/npm/category/trending |
| LibHunt         | https://www.libhunt.com                  |
| npm trends      | https://npmtrends.com                    |
| Star History    | https://star-history.com                 |
| OSS Insight     | https://ossinsight.io/                   |
| Good AI List    | https://goodailist.com/repos             |

**Good AI List API** — 16K+ AI open-source repos with star velocity, categories, and contributor data. No RSS; JSON API only.

| Endpoint | Returns |
| -------- | ------- |
| `/api/repos?page=1&page_size=N&sort=stars&order=desc` | Paginated repos (stars, 1d/7d velocity, forks, category, top devs, country) |
| `/api/repos/filters` | Available categories & subcategories |
| `/api/summary` | Total repos/devs/stars counts |

Categories: AI Engineering, Applications, Infrastructure, Lists, Models, Model Development, Tutorials, Misc

### GitHub Trending RSS

`mshibanami/GitHubTrendingRSS` — daily-generated Atom feeds hosted on GitHub Pages, one per language plus `all`.

| Period  | Pattern                                                                  | Example                                                                    |
| ------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------- |
| Daily   | `https://mshibanami.github.io/GitHubTrendingRSS/daily/{language}.xml`   | `mshibanami.github.io/GitHubTrendingRSS/daily/all.xml`                    |
| Weekly  | `https://mshibanami.github.io/GitHubTrendingRSS/weekly/{language}.xml`  | `mshibanami.github.io/GitHubTrendingRSS/weekly/typescript.xml`            |
| Monthly | `https://mshibanami.github.io/GitHubTrendingRSS/monthly/{language}.xml` | `mshibanami.github.io/GitHubTrendingRSS/monthly/python.xml`               |

Key feeds: `daily/all.xml`, `daily/python.xml`, `daily/typescript.xml`, `daily/rust.xml`

### Release Feed Pattern

`https://github.com/{owner}/{repo}/releases.atom`

Examples: `biomejs/biome`, `evanw/esbuild`, `vercel/turborepo`, `honojs/hono`

### Validation Tools

- `ghSearchRepos` — candidates by stars/topic/push date
- `ghViewRepoStructure` — changelog/release docs
- `ghGetFileContent` — release notes and changelogs
- `ghSearchCode` — validate specific deprecations or APIs
- Check merged PRs — prove shipped work

**High-value categories:** AI tooling, TS/JS tooling, build tools, browser frameworks, infra/observability, security tools, DB tools, CLI tools

---

## Cross-Domain Discovery

Initial scan and cross-domain signal catching. Does not replace domain-primary verification.

### Tier 1 (scan early)

| Source       | URL                                 | RSS                                       |
| ------------ | ----------------------------------- | ----------------------------------------- |
| Hacker News  | https://news.ycombinator.com        | `hnrss.org/frontpage`                     |
| Techmeme     | https://www.techmeme.com/           | `techmeme.com/feed.xml`                   |
| TechCrunch   | https://techcrunch.com/             | `techcrunch.com/feed/`                    |
| Reuters Tech | https://www.reuters.com/technology/ | —                                         |
| CNBC Tech    | https://www.cnbc.com/technology/    | —                                         |
| WIRED        | https://www.wired.com/              | `wired.com/feed/rss`                      |
| The Verge    | https://www.theverge.com/           | `theverge.com/rss/index.xml`              |
| Ars Technica | https://arstechnica.com             | `feeds.arstechnica.com/arstechnica/index` |

### Tier 2 (broad scan)

| Source            | URL                               | RSS                                       |
| ----------------- | --------------------------------- | ----------------------------------------- |
| MIT Tech Review   | https://www.technologyreview.com  | `technologyreview.com/feed/`              |
| VentureBeat       | https://venturebeat.com           | `venturebeat.com/feed`                    |
| Bloomberg Tech    | https://bloomberg.com/technology/ | `feeds.bloomberg.com/technology/news.rss` |
| TLDR Tech         | https://tldr.tech                 | —                                         |
| Hacker Newsletter | https://hackernewsletter.com      | —                                         |
| Lobsters          | https://lobste.rs                 | `lobste.rs/rss`                           |
| Hacker Noon AI    | https://hackernoon.com/tagged/ai  | `hackernoon.com/tagged/ai/feed`           |

### Cross-Domain Signals

| Source             | URL | RSS                                       |
| ------------------ | --- | ----------------------------------------- |
| SANS ISC           | —   | `isc.sans.edu/rssfeed.xml`                |
| Trail of Bits Blog | —   | `blog.trailofbits.com/index.xml`          |
| Rapid7 Blog        | —   | `rapid7.com/rss.xml`                      |
| Unit 42            | —   | `unit42.paloaltonetworks.com/feed/`       |
| Dark Reading       | —   | `darkreading.com/rss.xml`                 |
| The Register       | —   | `theregister.com/headlines.atom`          |
| Slashdot           | —   | `rss.slashdot.org/Slashdot/slashdotMain` |

### Tier 3 (supplemental)

TechURLs, Gizmodo (`gizmodo.com/feed`), daily.dev, Product Hunt, Exploding Topics, Google Trends

**AI-specific cross-domain feeds:** `techcrunch.com/category/artificial-intelligence/feed/`, `venturebeat.com/category/ai/feed/`

---

## Awesome Lists (Catalog Expansion)

Not part of the default execution path.

| Repo                                      | Coverage                                       |
| ----------------------------------------- | ---------------------------------------------- |
| `plenaryapp/awesome-rss-feeds`            | RSS/OPML collections (canonical)               |
| `foorilla/allainews_sources`              | 200+ AI/ML/Data Science sources with RSS URLs  |
| `finaldie/auto-news`                      | LLM-powered multi-source news aggregator       |
| `alvinreal/awesome-opensource-ai`         | Curated open-source AI projects and tools      |
| `vishalshar/awesome_ML_AI_RSS_feed`       | ML/AI RSS feeds                                |
| `wyattowalsh/ai-web-feeds`                | Categorized AI OPML files + enriched feed data |
| `watchstep/ai-weekly-report`              | Clean OPML (Official/Community/Papers)         |
| `kilimchoi/engineering-blogs`             | Engineering blog directory + OPML              |
| `zudochkin/awesome-newsletters`           | Newsletter master list                         |
| `vitalets/github-trending-repos`          | Trending repo notifications                    |
| `TalEliyahu/awesome-security-newsletters` | Security newsletters                           |
| `clivoa/awesome-security-feeds`           | Structured security RSS/Atom collection        |
| `alternbits/awesome-ai-newsletters`       | AI newsletters                                 |
| `duanyytop/agents-radar`                  | AI agent ecosystem tracker                     |
| `anuj0456/ailert`                         | 230+ AI source aggregator                      |
| `DongjunLee/awesome-feeds`                | Tech, ML, and business feeds                   |
