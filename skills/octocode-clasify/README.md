# Octocode Clasify

Judge unread local or GitHub files without fetching their bodies into chat. Each judgment is one typed question per resource: **Noul** (probability 0–1), **Choice** (one named class), or **Score** (one ordered level).

## Use when

First identify an unresolved judgment that changes the next action. Skip when current evidence, direct reasoning, or a cheap exact check already decides; size and candidate count alone are not triggers.

- You have several candidate files and need to triage which to read, without reading all of them.
- You need to route a page or artifact to `read` / `skip` / `consider` before it enters context.
- A bounded yes/no, classification, or ordering judgment over supplied or unread resources changes the next action.

## Not for

- Exact or deterministic checks (file exists, regex match, version equals) → use `localSearch`, `astSearch`, or a direct test
- Verifying literal numeric or string presence in text → use `localSearch` with `regex:"literal"`
- Free-form summarization or full-body extraction → read the file directly
- Settled decisions with known answers → no semantic call needed

## Question types

| Type | Returns | Use for |
|---|---|---|
| `noul` | probability 0–1 | yes/no questions: does this supplied content support a scoped proposition |
| `choice` | one named class | routing: `read`/`skip`/`consider`, content type, link action |
| `score` | one ordered level | quality/priority ranking |

For research checks, choose `questionType:"contribution"` with a `target`, `"addsEvidence"` with a `target` and a small `knownEvidence` ledger, or `"supportsClaim"` with the exact claim as `target`. Each preset expands to one Noul question.

For other judgments, supply custom `type`, `instructions`, and optional `criteria` unchanged. Do not mix custom fields with a preset. Clasify adds no implicit labels, localization questions, or routing flags. Add an explicit abstention class to Choice when needed. Validated probability distributions retain their full precision.

**Confidence:** distribution concentration is not correctness. No universal threshold safely discards a candidate. Retain uncertain, `insufficient`, or incomplete results for a deciding read; verify factual claims against source evidence.

## Workflow

Use direct execution with the current CLI form. This complete shape is valid for one unread retained artifact; replace the absolute path and target:

```bash
octocode clasify '{"id":"artifact-locate","reasoning":"Locate an answer before reading the artifact.","resources":[{"id":"saved-page","context":{"tool":"localFetch","query":{"reasoning":"Assess the retained artifact without returning its body.","path":"/ABS/.octocode/tmp/scrape/session/text/page-001.clean.part-001.md","fullContent":true}}}],"questions":[{"id":"answer","questionType":"locate","target":"What does the Choice primitive return?"}]}'
```

Each matrix is `{id, reasoning, resources:[{id, context}], questions:[{id, questionType:"locate", target}]}`. Every question is evaluated independently for every resource, so put competing candidates in one matrix to screen them in parallel with one focused question. Maximum 25 resource×question cells. Batch unrelated matrices under root `queries[]`; dependent questions require a later call.

One `localSearch` or GitHub code-search resource fans its returned file entries into independently judged pages. Omit `candidateEvidence` (or use `"search"`) to judge only paths, snippets, and metadata. Use `candidateEvidence:"fileChunks"` when snippets cannot route the next read: the runtime hydrates up to five bounded candidate chunks per call and returns `page.next.read`, without returning bodies. Follow `next.clasify` for later search pages and verify selected source before making factual claims.

## Install

```bash
npx -y octocode skill install octocode-clasify
```

Requires `OCTOCODE_CLASSIFICATION_API` in the environment.

## Related skills

- `octocode-research` — gathers and verifies source evidence
- `octocode-scraping` — locates semantic answers in unread saved pages
- `octocode-chrome-devtools` — locates controls or facts in unread saved captures

## References

- [Protocol and query schema](references/ojql.md)
- [Workflow patterns](references/clasify-workflows.md)
