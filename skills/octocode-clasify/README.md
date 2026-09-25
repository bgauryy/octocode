# Octocode Clasify

Judge unread local or GitHub files without fetching their bodies into chat. Each judgment is one typed question per resource: **Noul** (probability 0–1), **Choice** (one named class), or **Score** (one ordered level).

## Use when

First identify an unresolved judgment that changes the next action. Skip when current evidence, direct reasoning, or a cheap exact check already decides; size and candidate count alone are not triggers.

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
| `noul` | probability 0–1 | yes/no questions: does this supplied content support a scoped proposition |
| `choice` | one named class | routing: `read`/`skip`/`consider`, content type, link action |
| `score` | one ordered level | quality/priority ranking |

**Confidence:** distribution concentration is not correctness. Set routing thresholds for the cost of a wrong decision. Retain uncertain, `insufficient`, or incomplete results for a deciding read; verify factual claims against source evidence.

## Workflow

```bash
octocode clasify --input request.json --compact
```

Each query: `{id, reasoning, resources:[{id, context}], questions:[{id, question}]}`. Every question sees every resource. Maximum 25 cells per query. Batch independent matrices under root `queries[]`.

## Install

```bash
npx -y octocode skill install octocode-clasify
```

Requires `OCTOCODE_CLASSIFICATION_API` in the environment.

## Related skills

- `octocode-research` — uses clasify for file triage during investigation
- `octocode-scraping` — uses clasify when unread corpus relevance is ambiguous
- `octocode-chrome-devtools` — uses clasify for DOM/HAR artifact screening

## References

- [Protocol and query schema](references/ojql.md)
- [Workflow patterns](references/clasify-workflows.md)
