# Octocode Clasify

Judge unread local or GitHub files without fetching their bodies into chat. Each judgment is one typed question per resource: **Noul** (probability 0–1), **Choice** (one named class), or **Score** (one ordered level).

## Use when

- You have several candidate files and need to triage which to read, without reading all of them.
- You need to route a page or artifact to `read` / `skip` / `consider` before it enters context.
- A bounded yes/no, classification, or ordering judgment over supplied or unread resources changes the next action.

## Not for

- Exact or deterministic checks (file exists, regex match, version equals) → use `localSearch`, `corpus-run`, or a direct test
- Verifying literal numeric or string presence in text → use `corpus-run --regex`
- Free-form summarization or full-body extraction → read the file directly
- Settled decisions with known answers → no semantic call needed

## Question types

| Type | Returns | Use for |
|---|---|---|
| `noul` | probability 0–1 | yes/no questions: is this relevant, is this JS-rendered, does this contain pricing data |
| `choice` | one named class | routing: `read`/`skip`/`consider`, content type, link action |
| `score` | one ordered level | quality/priority ranking |

**Confidence gate:** accept a `choice` route only when `confidence >= 0.5`. Treat anything lower as `consider`.

## Workflow

```bash
octocode clasify --input request.json --compact
```

Each query: `{reasoning, resources:[{id, context}], questions:[{id, question}]}`. Every question sees every resource. Maximum 25 cells per query. Batch independent matrices under root `queries[]`.

## Install

```bash
npx -y octocode skill install octocode-clasify
```

Requires `OCTOCODE_CLASSIFICATION_API` in the environment.

## Related skills

- `octocode-research` — uses clasify for file triage during investigation
- `octocode-scraping` — uses clasify for corpus SCREEN before reading pages
- `octocode-chrome-devtools` — uses clasify for DOM/HAR artifact screening

## References

- [Protocol and query schema](references/ojql.md)
- [Workflow patterns](references/jev-workflows.md)
