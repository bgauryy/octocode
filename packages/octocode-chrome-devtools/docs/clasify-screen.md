# Scout unread browser evidence

> MCP note: the default `--mcp` surface is the research preset. Outside it (`check` and the `helper` tool that runs `har-pager`, `measure-query` and `snapshot-query`), use `--mcp --preset full` or the typed `/cli` and `/raw` commands.

Load when many plausible capture passages, resources or images remain after exact filtering. Browser tools preserve evidence and execute actions; Octocode retrieves it locally; optional JEV classification decides what the host should inspect. A small known answer needs ordinary retrieval only.

## Pick the cheapest deciding evidence

| Evidence and intent | First narrow with | Use JEV only for the remaining ambiguity | Verify with |
|---|---|---|---|
| DOM or accessibility controls | Role, accessible name, state, frame and ref | Which described region/control offers the requested operation | Complete node/ref plus current DOM and action postcondition |
| Accessibility review | Missing names, labels, explicit relationships and deterministic checks | Candidate wording or visible-content mismatches needing inspection | DOM/AX properties, keyboard/focus checks and applicable accessibility criteria |
| HAR or network | URL, method, status, MIME, time and request ID | Which readable bodies state a requested condition | Original entry/body and request/frame/session identity |
| Large articles, logs or documentation | Source metadata, headings, paths and literal hits | Which passages supply a requested fact or prerequisite | Selected original passages, URL and coverage |
| JavaScript | Source URL, source map, symbol or exact string | Which readable candidate excerpts contain an observable check or handler | Exact code, callers and runtime evidence |
| Screenshots or site images | Asset metadata, DOM location and admitted image dimensions | Visible state, object presence or simple visible relationship | Pixels when needed; current DOM for actions |

Classification cannot establish WCAG conformance, keyboard behavior, reachability or causality. Use DOM IDs and properties for label/control relationships, request IDs/initiators/timestamps for network relationships, and the host to reason across those records. Images can suggest visual relationships; a suggested box does not identify a live DOM element.

## Capture and narrow before sending references

1. Capture once. `SNAPSHOT_STDOUT=summary` keeps large snapshot refs on disk; if returned refs or previews already answer, stop. Save source URL, frame and stable record identity with the complete evidence.
2. Response-content questions need `network-body-har-fetch-check` with the observed `BODY_URL`/`BODY_MATCH`, or suitable raw CDP body capture; see `cdp-checks`. `run` network observation and `live-har-monitor` capture metadata only. Subscribe before the relevant load/action; if missing bodies cannot be recaptured, keep that gap explicit.
3. Use `query` for saved JSON/JSONL, `helper` with `snapshot-query` for controls, and `har-pager`/`measure-query` for their supported record views. Filter before paging. These return structured pages; they do not automatically export decoded prose for classification.
4. Use the returned `files`/`evidence` paths with Octocode `localSearch`, with `includeHidden`, `includeIgnored` and `defaultExcludes: false`. Narrow with literal terms before adding a classifier. Use `localFetch` for a known span or a large source whose relevant location remains unknown.
5. For escaped, minified or oversized records, create a readable capture or deterministic projection with the existing script API (`script-patterns`). Preserve original bytes and map every projected record/line back to its original pointer, URL and ID. Decode only an identified field/encoding; a projection is not the original record. Save it directly to disk so its body need not enter the host first. A missing HAR body is unavailable evidence, not an empty response.

Do not send raw credential-bearing HARs. Select the authorized fields/bodies before external classification. Large binary files and PDFs are not text sources: extract text or render admitted image pages with an appropriate tool and preserve page mapping. Direct PDF, audio and video classification is not established.

## Choose one classification stage

**Need passages from a known capture:** give `clasify` a `localSearch` over the capture as its `source`, with `predicates`. It runs the search and answers each hit (with its context lines) as `p1`, `p2`, … under the hit's file and line, with `coverage` and a `next.read` for the deciding lines. This `clasify` request judges the hits of a literal match:

```json
{"queries":[{
  "operation":"evaluate",
  "source":{"tool":"localSearch","query":{
    "path":"/absolute/captures/response-bodies.txt",
    "matchString":["session"],
    "contextLines":{"before":4,"after":4}
  }},
  "predicates":["This response explicitly says the current upload session has expired and cannot accept more data."]
}]}
```

Choose observed terms and enough context for the predicate. Literal retrieval can miss synonyms; classification cannot recover text the search excluded. A hit's window may include adjacent records: verify each deciding record rather than treating the whole window as a positive answer. Windows are bounded, not whole documents.

A high answer marks a hit to read, not a fact. A window can include adjacent counterevidence, so read the deciding record (`next.read`) before answering. For a yes/no decision about a complete unread record, ask a `noul` question on its source reference after structural narrowing. Small records may cost less to read directly. Inspect `coverage`: quiet, undecided and failed units stay unresolved (`next.quiet`, `next.retry`). For an exhaustive task, unresolved hits still require inspection; do not claim savings from hiding them.

**Need to choose which unread resources to read:** batch native `clasify` rows with `source` references. Ask independent questions about the same source in one row. Use `npx -y octocode schema clasify --view query` to check the installed contract; run a saved request with `npx -y octocode clasify --input request.json`:

```json
{"queries":[{
  "operation":"evaluate",
  "source":{"tool":"localFetch","query":{"path":"/absolute/captures/guide.txt"}},
  "questions":{
    "replacement":{"type":"noul","instructions":"Does this guide explicitly require replacing an expired upload session before resuming?"},
    "integrity":{"type":"noul","instructions":"Does this guide explicitly require checking file integrity before resuming an upload?"}
  }
}]}
```

Group resources as separate rows, within live limits. Each question must stand alone: sibling answers and the host conversation are unavailable. Send paths/URLs/assets, not previously read bodies copied into `content`. `content` holds literal data or image keys; a URL string inside it is not fetched. Standalone answers route inspection; the `read` suggestion in `hints`, when returned, covers the captured selection rather than answer-specific lines. Read a smaller observed anchor when sufficient.

## Ask questions that change the next read

- **Noul:** one stated or visible predicate, such as whether a body explicitly reports expired authorization. Keep inferred causes and safety verdicts with the host.
- **Choice:** distinct supplied outcomes plus an insufficient-evidence outcome when needed. Known region labels can route inspection; the model cannot invent selectors or enumerate every matching location.
- **Score:** meaningful ordered evidence levels. It is not an assumed 1–10 correctness scale or calibrated confidence.
- **Box:** pair localization with an independent presence question in the same row; check that answer before using coordinates. Coordinates alone do not prove that the object exists or permit an action.

For one image, name the PNG or JPEG as the row's `source` (`{"tool":"localFetch","query":{"path":"/absolute/screenshot.png"}}`), or list several under `resources` for an answer set per image; use a tested image-capable model. One row supports independent presence, state and location questions. To compare two images in one judgment, put both under `assets` and name both asset keys in `content`; retain each capture time, viewport and page identity. A visible difference does not explain its cause. Page image URLs must be discovered explicitly; an HTML text source does not crawl its images.

Judge a captured page from its saved text (`page-text.txt`) or its URL, not from its HTML: on one docs page the HTML cost 75,856 input tokens against 2,720 for the extracted text, with the same answers. A HAR that fits one window is cheapest judged whole with several questions; use `operation:"sections"` when every matching request is needed.

Prefer putting each image reference once in a named content field and asking questions about that field. Repeating the raw asset reference repeats the image in provider context; use distinctive asset keys to avoid accidental matches. Questions comparing resources need their evidence in the same evaluation row; image references can also appear in instructions or criteria. Sibling rows do not share evidence. For related text records, save a bounded combined capture with each record's provenance before asking a joint question.

## Verify, recover and stop

If the evidence read answers the scoped question, stop without classifying it again or fetching the same body. Follow `next.nextPage` when more judged rows are needed. Unknown/error is not irrelevant: recover the underlying source when it can change the answer or completeness is required. Run the same search exactly, without `clasify`, only when it adds missing evidence; known candidate paths may permit a direct deciding read instead. Otherwise state the unresolved scope.

Classification availability depends on the configured provider and its successful ping; beta is not required. Keep ordinary retrieval available. Correct an input rejection before retrying; inspect the saved image/source when classification fails. Do not switch models silently or turn a low score into an absence claim.

Before acting, confirm the target in the current DOM/frame and verify the action's postcondition. Measure complete host input/output including classification, recovery and duplicate reads; small classifier answers alone do not prove savings.

Next: act on the confirmed target using `browser-execution` or `cdp-checks`; use `recovery` for uncertain mutations.
