---
name: octocode-clasify
description: "Use when an explicit classification request needs a typed judgment, or when a semantic answer should be located inside an unread known file before the host reads it. The locate flow spends cheap provider tokens and returns one small ranked verification window plus P(answer); use direct search for literals. Batch independent same-evidence questions in one matrix. Supports unread Scout resources, saved scrape/browser artifacts, and supplied-state judgments; not proof, missing facts, or summaries."
---
# Clasify

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise use the live `scheme clasify` contract for fields.

Clasify is the only semantic tool. Jev is its provider, not another tool. The two modes are:

- **Scout**: screen unread read-tool resources; receive typed judgments and source scopes, never source bodies.
- **Judge**: classify state already held in `context.value`; no retrieval occurs.

## Admission

Use Clasify for an explicit classification request or for `questionType:"locate"` over unread known files when the target is semantic rather than a useful literal. Locate is the only research flow currently shown to reduce the complete host-visible result: in the held-out external-doc run it returned verification windows with 20.4% fewer response bytes than search-first. This is a wire-byte result, not measured host-token proof. Direct search remains the default for literals and already-known anchors.

The optimization target is host-model context at acceptable quality. Jev/provider tokens are a separate, cheaper budget: spending more of them is useful when it prevents larger host-visible body reads. Record provider usage and latency, but do not reject a flow merely because provider tokens rise.

Skip it when an exact check, current evidence, direct reasoning, or a cheap bounded read decides the action. Candidate count, file size, one search miss, or shorter response text alone is not an admission reason. Every call still adds latency and verification work.

## Workflow

1. State the unresolved decision and what each outcome changes.
2. Repair scope, spelling, filters, and synonyms before classifying search results. On large repositories, narrow the local root/include/exclude filters or GitHub owner/repo/filename/language first. A miss does not prove absence.
3. For unread `localSearch` or GitHub code-search results, send the search request as one resource. Omit `candidateEvidence` (or set `"search"`) to judge only returned paths, snippets, and metadata. Set `candidateEvidence:"fileChunks"` only for an explicit experiment when snippets cannot route the next read; the five-bug agent A/B did not save host tokens. For known `localFetch` or `ghGetFileContent` files, including retained scrape text and Chrome snapshots, use one resource per file and `questionType:"locate"`: the runtime tags source passages, asks one Choice plus one Noul per target, and returns the strongest bounded verification window plus `exists`. Ask one atomic target per question. Put all independent targets that use the same files in this matrix so each body is captured once. Expanded pages count toward the 25-cell limit.
4. Use Scout only when the visible paths, metadata, snippets, and direct reasoning cannot choose the next read. Use complete, bounded sections; do not submit arbitrary prefixes or huge files.
5. Use Judge when held evidence remains ambiguous and its disposition changes the next read, test, or edit. Include only the smallest sufficient observations, constraints, counterevidence, and uncertainty.
6. Read the deciding source after a routing judgment. For locate, use `source.path` with `matches[0].startLine/endLine`; widen the verification read slightly if a record or sentence crosses that window. Treat a high-ranked window with low `exists` as “closest passage,” not an answer. Reuse a judgment only for the same question, evidence, and model identity. Do not automatically chain Scout → Judge.

Use one matrix for the cross-product only when every question applies to every resource. Use root `queries[]` for independent matrices with different evidence sets. Dependent questions belong in a later call after the first answer changes the evidence or available options.

Measure leverage as final quality plus actual host tokens. A useful intermediate diagnostic is `(direct-read host bytes − Clasify result bytes − required verification-read bytes) / provider input tokens`; label it a wire-byte proxy, never actual host-token proof. Read selected, conflicting, and uncertain candidates—not every screened file—unless completeness requires all of them.

## Question rules

- Each question asks one atomic thing. Several independent questions may share a matrix when they use the same evidence. Put the decision and constraints in `instructions`; `reasoning` is trace metadata and is not provider evidence.
- `noul` returns P(yes) for one proposition. It is not a proof of truth or a measure of intensity.
- `choice` returns exactly one caller-defined label and its full probability map. Add `insufficient` when none of the substantive labels may be safe. Keep `supported`, `contradicted`, and `conflicting` distinct when that changes the action.
- `score` orders one dimension with self-contained levels. It is an expected level, not correctness.
- Confidence is distribution concentration, not correctness. There is no universal discard threshold; retain uncertain, mid-band, insufficient, partial, and errored candidates for a deciding read.
- For research presets use `locate`, `contribution`, `addsEvidence`, or `supportsClaim`. Locate accepts only `target` and is valid only for one contiguous original-source file view. Do not mix a preset with custom `type`, `instructions`, or `criteria`.

## Input shape

For unread evidence, use `context.tool` beside one direct `context.query`; do not copy the body into `context.value` and do not nest `queries[]`. Use an absolute local path or GitHub `owner`, `repo`, `path`, and optional `branch`. For held evidence, use `context.value`. This executable form was verified against retained scrape text; replace `ARTIFACT` with your file:

```bash
ARTIFACT="$PWD/.octocode/tmp/scrape/<session>/text/page-001.clean.part-001.md"
octocode clasify "{\"id\":\"artifact-locate\",\"reasoning\":\"Locate two answers before reading the retained artifact.\",\"resources\":[{\"id\":\"saved-page\",\"context\":{\"tool\":\"localFetch\",\"query\":{\"reasoning\":\"Assess the retained artifact without returning its body.\",\"path\":\"$ARTIFACT\",\"fullContent\":true}}}],\"questions\":[{\"id\":\"choice-output\",\"question\":{\"questionType\":\"locate\",\"target\":\"What does the Choice primitive return?\"}},{\"id\":\"score-output\",\"question\":{\"questionType\":\"locate\",\"target\":\"What does the Score primitive return?\"}}]}"
```

Use `scheme clasify --view query --compact` for the live schema and executable form. The current CLI is `octocode clasify '<json>'`; in this repository use `node packages/octocode/out/octocode.js clasify '<json>'`.

## Results and verification

Pages carry answers keyed by question ID plus coverage and source/scope/view receipts. Preserve resource IDs and disjoint source ranges. A transformed view is not a source line range; verify citations against the original source. `source.evidenceHash` identifies the assessed capture, not immutable file bytes.

A locate answer is `{exists,matches:[{startLine,endLine,probability}]}`. The Choice probability ranks the generated passage within that page; `exists` independently estimates whether the page answers the target. The match is a small verification window around the ranked passage, not a parsed syntax boundary. Compare `exists` and match probability across candidate resources, then read the deciding windows. Partial page coverage cannot prove file-wide absence.

For a `localSearch` or GitHub code-search resource, each returned file entry is one page with its own `source.path` and answer. In snippet mode the answer covers only the returned path, snippets, and metadata. In `fileChunks` mode it covers one bounded candidate chunk. A hydrated page with a usable file identity exposes executable `next.read`; failures may not. Search-page continuation remains in `next.clasify`; copy it unchanged to assess later result pages. Clasify keeps every candidate and score—it does not apply a hidden threshold.

Follow `next.clasify` unchanged when more coverage remains. Exit 6 means a continuation is available. `partial`, `error`, `insufficient`, or a mid-band result means narrow or read; it does not mean “no.” Content-firewall rejection is also unresolved. Never infer identity, reachability, absence, or mutation safety from a verdict.

Other tools return evidence only; they never call the classifier. Other skills defer admission to this skill. Do not add `jev`, `jevScout`, `jevReasoning`, or `semanticAssess` aliases.

For protocol details see [ojql.md](references/ojql.md). For optional patterns see [clasify-workflows.md](references/clasify-workflows.md).
