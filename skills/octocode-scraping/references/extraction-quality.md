# Extraction Quality

Load when turning fetched pages into facts, rows, or summaries. Why: scraped data is noisy and must be auditable.

## Before extraction
- Define the target schema or claim list.
- Record route, source URL, status, content type, and fetch time in `sources.jsonl`.
- Prefer normalized `text/` and `extracts/` for search; keep `raw/` for audit.

## Validate
- Emit counts and 3–5 sample rows for structured extraction.
- Spot-check critical facts against raw HTML or source snippets.
- Cross-check important claims with another selector/source when possible.
- Mark partial, blocked, transformed, or inferred data explicitly.

## Output quality
- Store rows as JSONL in `extracts/`.
- Store compact evidence excerpts in `snippets/`.
- Store the narrative in `reports/summary.md`; link artifact paths, not giant pasted data.
- Do not treat Markdown transformation or AI summaries as authoritative without exact source evidence.

Next: for the exact file and stdout shapes you cite load `references/data-contract.md`; to locate the evidence on disk load `references/session-corpus.md`.
