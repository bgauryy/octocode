# Octocode Clasify

Judge unread local or GitHub files without fetching their bodies into chat. Each judgment is one typed question per resource: **yesno** (probability 0–1), **choice** (one named class), **score** (one ordered level), or **locate** (ranked source line windows).

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

| `type` | Returns | Use for |
|---|---|---|
| `yesno` | probability 0–1 | yes/no questions: does this supplied content support a scoped proposition |
| `choice` | one named class | routing: `read`/`skip`/`consider`, content type, link action |
| `score` | one ordered level | quality/priority ranking |
| `locate` | ranked line windows | where an unread file answers a described target |

Every question is `{type, ask}`. For research checks, choose `type:"relevant"` (contribution), `"adds"` with a small `known` ledger, `"supports"` with the exact claim as `ask`, or `"sufficient"`. Each expands to one yes/no provider question.

Custom judgments put their meanings in `labels` (choice `{label: meaning}`, score `[low … high]`, optional yesno `{true, false}`). Clasify adds no implicit labels, localization questions, or routing flags. Add an explicit abstention class to choice when needed. The older `questionType`/`target`, `type`/`instructions`/`criteria`, and nested `context` forms still validate.

**Confidence:** distribution concentration is not correctness. No universal threshold safely discards a candidate. Retain uncertain, `insufficient`, or incomplete results for a deciding read; verify factual claims against source evidence.

## Workflow

Use direct execution with the current CLI form. This complete shape is valid for one unread retained artifact; replace the absolute path and target:

```bash
octocode clasify '{"id":"artifact-locate","goal":"Find what the Choice primitive returns in the saved page.","reasoning":"Locate an answer before reading the artifact.","resources":[{"id":"saved-page","tool":"localFetch","query":{"path":"/ABS/.octocode/tmp/scrape/session/text/page-001.clean.part-001.md"}}],"questions":[{"id":"answer","type":"locate","ask":"What does the Choice primitive return?"}]}'
```

Each matrix is `{id?, goal, reasoning, resources:[{id?, tool, query} | {id?, value}], questions:[{id?, type, ask}]}`; `goal` and `reasoning` are required, omitted IDs are derived by position. Every question is evaluated independently for every resource, so put competing candidates in one matrix to screen them in parallel with one focused question. Maximum 25 resource×question cells. Batch unrelated matrices under root `queries[]`; dependent questions require a later call.

One `localSearch` or GitHub code-search resource fans its returned file entries into independently judged pages. Screening questions judge only paths, snippets, and metadata. A `locate` question (or `candidateEvidence:"fileChunks"`) hydrates up to five bounded candidate chunks per call and returns `page.next.read`, without returning bodies. Follow `next.clasify` for later search pages and verify selected source before making factual claims.

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

- Protocol and query schema: live `octocode scheme clasify --view query --compact`
