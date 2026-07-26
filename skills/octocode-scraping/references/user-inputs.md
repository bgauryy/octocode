# User Inputs

Load before broad crawling, extraction schemas, or workflow analysis. Why: better inputs produce smaller, safer corpora and clearer answers.

## Ask for
- Goal: summarize, extract rows, understand navigation, find docs/API/pricing, monitor changes, or prove a claim.
- Scope: one URL, explicit URL list, same-domain crawl, sitemap crawl, max pages, and delay.
- Output shape: bullets, table, JSONL schema, graph summary, or cited answer.
- Evidence strictness: quick scan, cited facts, raw audit, or cross-source validation.
- Boundaries: auth, personal data, forms, CAPTCHA/MFA, rate limits, and forbidden areas.

## Default if user is vague
Use one public URL, markdown mode, no auth, no broad crawl, default `.octocode/tmp/scrape/{sessionId}`, and return only the session path + next search targets.

## Mode mapping
| User intent | Route |
|---|---|
| “fetch/read this page” | `--mode markdown` |
| “need raw/source” | `--mode html` |
| “JS rendered element” | `--mode html --browser --wait-for <selector>` |
| “structured fields” | `--mode extract --extract-properties <schema>` |
| “understand site/workflows” | bounded `--crawl --same-domain --max-pages <n>` then graph/index analysis |
