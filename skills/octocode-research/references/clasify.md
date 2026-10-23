Load when choosing unread evidence, using `clasifyContext` or `clasify`, or recovering from unknown/error results. Check the chosen tool's live schema and availability.

## Choose the route

| Next decision | Route |
|---|---|
| A known literal, line or small file answers it | Ordinary search or bounded fetch |
| Broad lexical search leaves many plausible hits | Narrow path/globs/known file terms, then optional `clasifyContext` |
| Exact filtering of known files still leaves ambiguous passages | Batch classified fetches; receive selected evidence immediately |
| Decide which unread files, documents or images deserve a read | `clasify` with source/asset references; receive typed answers only |
| Ask shared questions about individual text regions across resources | `clasify operation:"sections"`, when exposed by the live schema |
| Every match, absence, symbol identity or reachability | Ordinary traversal, AST/LSP and exact source proof |

For browser evidence and actions, follow the [Chrome workflow](../../octocode-chrome-devtools/SKILL.md); load its `clasify-screen` topic for DOM, network and image requests.

Before calling, identify the read that a judgment could avoid. Discover paths with structure/search first; a folder is not one `source`. Need passages now: classified fetch. Need labels to choose resources: standalone `clasify`. Send paths/URLs/assets instead of reading bodies into the host to construct `state`. Use ordinary retrieval if classification is unavailable or exact filtering already settles the question. Compare total host input/output, including prompts, tool arguments and recovery reads; smaller tool output alone does not establish savings.

## Select inside search and fetch

Use `clasifyContext:{classifiers:["one self-contained evidence question"],longContext?:true}`. Supply 1–5 atomic questions naming the subject and deciding conditions. Optional per-row `mainGoal` and `reasoning` reach every inline question; reasoning is a short search rationale. JEV sees neither the conversation nor answers to sibling questions.

| Tool | Evidence JEV sees |
|---|---|
| `localSearch` | Sanitized hit lines and their supplied context, before output paging |
| `ghSearchCode` | Experimental: listed hit lines (up to 5 per file) on this provider page; filename-only candidates stay unknown. `next.judgeHits` judges the unlisted hits of hidden files. |
| `localFetch`, `ghGetFileContent` | Source-addressed windows from the extracted ranges/matches or file, before output paging |

Inline search does not read hidden whole-file bodies. For localSearch, apply cheap `include`/`exclude` and known `fileHas`/`fileLacks` terms first. With clasifyContext, each localSearch hit is judged with ±8 surrounding lines by default; set `contextLines` only to change that (larger for a distant deciding condition, 0 for bare hit lines). `responseLength` changes output page size, not the evidence JEV sees. To inspect content behind an unread path, use standalone `clasify` or a classified fetch. Keep lexical scope broad enough to retrieve the deciding evidence; classification cannot recover a file the search never found.

```json
[
  {"tool":"localSearch","query":{"path":"src/http","matchString":"body","contextLines":3,"clasifyContext":{"classifiers":["Does this code reject request bodies that exceed the configured size limit?"]}}},
  {"tool":"localFetch","query":{"path":"docs/<upgrade-guide>.md","clasifyContext":{"classifiers":["Does this passage specify when a rolling upgrade may mix server versions?"]}}},
  {"tool":"ghGetFileContent","query":{"owner":"OWNER","repo":"REPO","path":"docs/<upgrade-guide>.md","ref":"OBSERVED_REF","clasifyContext":{"classifiers":["Does this passage specify when a rolling upgrade may mix server versions?"]}}}
]
```

Replace targets with discovered paths/refs. Omit fetch `minify` for context selection; fresh byte windows, explicit minification and streamed sources are unsupported. Local search requires a content view, not files/counts. `fullContent` is compatible but output still pages. `longContext:true` opts into the larger provider context; candidate/character/deadline budgets still apply and the provider may reject it.

For a known file with many literal hits, use fetch `matchString` plus enough `contextLines`, then classify that view. JEV cannot recover conditions outside it. Positive context may complete a small enclosing declaration; explicit ranges bound that expansion. Prefer ordinary matched fetch when those windows are already sufficient. `contextLines:0` explicitly requests complete matching lines, including very long lines. Original line gutters and `sourceLineRanges` remain file coordinates; never count output rows as source lines.

Whole-file fetch keeps declarations and their documentation together when they fit the window budget. Oversized declarations, source gaps, unsupported syntax and extracted views use source-addressed line windows targeting 2,048 characters. Implicit long-line excerpts retain omission markers and original coordinates. A window does not guarantee all the context a question needs. For a predicate that needs joint context across windows, use a complete standalone source selection if a typed answer can avoid reading, or inspect the deciding ranges together. `longContext` does not join inline windows or expand selection budgets. Large size alone is not a reason to classify: the question must distinguish useful material from background.

Successful selection returns selected bodies in source order; a window can include neighboring background. Inspect `selection` status, scope, unit and selected/excluded/unknown counts. `complete` means all candidates in that declared view were decided, not that the repository or question is complete. Failed, ineligible and budget-limited candidates stay unknown. A localSearch provider failure can return unfiltered hits with an unavailable selection; those hits are ordinary evidence, not classified selections.

Read returned evidence once. If it answers the question, stop. Otherwise follow needed selected-page continuations unchanged, narrow a new call to the missing fact, or use the [recovery rules](#recover-from-unknown-or-error). A changed question starts a new call; never edit a bound selection token.

## Screen unread sources with clasify

Each evaluation row needs keyed `questions` and exactly one of `state` or `source`; batch up to 5 independent resources. Questions about the same resource share one row. Keep standalone question intent self-contained: row metadata is not a substitute for `instructions`.

- `state`: data already held. A path or URL inside it is literal data and is never fetched.
- `source`: `{tool:"localFetch",query:{path,ranges | fullContent:true}}`, `{tool:"ghGetFileContent",query:{owner,repo,path,ref?,ranges | fullContent:true}}`, or `{url:"https://…#section"}`. File sources require an explicit complete selection. Octocode reads internally; no body reaches the host.
- Optional `longContext:true` raises the admitted state size. A too-large row fails rather than classifying a truncated prefix; narrow the selection or opt in deliberately.

```json
{"tool":"clasify","query":{"queries":[
  {"id":"upgrade-guide","source":{"tool":"localFetch","query":{"path":"docs/<upgrade-guide>.md","fullContent":true}},"questions":{"mixed_versions":{"type":"noul","instructions":"Does this guide explicitly state when a rolling upgrade may mix server versions?"}}},
  {"id":"deployment-guide","source":{"tool":"localFetch","query":{"path":"docs/<deployment-guide>.md","fullContent":true}},"questions":{"mixed_versions":{"type":"noul","instructions":"Does this guide explicitly state when a rolling upgrade may mix server versions?"}}}
]}}
```

Use the answers to choose the next read. For a source claim, read the deciding passage, preserving the returned snapshot/ref and selection scope. The read suggestion in `hints` identifies the captured selection; it does not require consuming a whole winning file when a smaller anchored passage suffices. Hints are strings containing guidance or executable tool/JSON suggestions. For a task requesting only typed classification, the answer may finish it. Save large requests under `<workspace>/.octocode/octocode-research/tmp/` and run `npx octocode clasify --input <file>`.

## Assess sections across resources

Use `operation:"sections"` when regional answers change the next read. Supply keyed `resources` and shared keyed `questions`; each resource uses the file/URL source forms above. Discover concrete paths first. Octocode captures and divides text internally, so the host need not read bodies to construct the request. Images use ordinary evaluation.

```json
{"tool":"clasify","query":{"queries":[{"operation":"sections","resources":{"request":{"tool":"localFetch","query":{"path":"src/<request-file>","fullContent":true}},"policy":{"tool":"localFetch","query":{"path":"src/<policy-file>","fullContent":true}}},"questions":{"size_check":{"type":"noul","instructions":"Does the target section compare request-body size against a configured limit?"},"rejection":{"type":"noul","instructions":"Does the target section reject a request when a configured limit is exceeded?"}}}]}}
```

Replace paths with discovered targets. Ask only questions needed for a decision: a complete resource × section × question matrix can cost more than a direct read. For selected passages alone, prefer classified fetch; for one answer about a whole source, use ordinary evaluation.

Results group sections under resources. Shared question definitions supply answer types and Score legends; retain each cell's status and complete distribution. `error`, `unsupported` and `unassessed` are unresolved, not negative answers. Coverage is cumulative across batches. Targets identify the judged byte/line range; supporting context is separate. Use the resource's `read` reference at its snapshot/ref to inspect deciding targets, including byte offsets for targets on the same long line.

Copy `responsePagination.next` to finish saved display pages without inference, then `next.nextSectionPage` when remaining assessment is needed. The work continuation can spend provider requests; settled work replay reports zero current usage. Display receipts are historical, so do not sum them as new spending. Preserve unknown totals and terminal reasons; an exhausted budget does not prove absence. Local capture and retained plans are bounded: follow an explicit size recovery instead of assuming streaming covers the whole source.

## Locate before reading

A source row's read suggestion in `hints` covers everything judged, not the supporting lines for each answer. To choose a smaller read, use `choice` with labels describing known original line ranges and an `unknown` label, plus an independent `noul` question asking whether the selected source addresses the fact at all. Send source paths; do not copy their bodies into questions. Use coarse ranges before enumerating hundreds of line labels, then read the chosen range with needed context at the same snapshot/ref.

Choice locates one preferred passage; probabilities compete and do not identify every relevant site. Use classified fetch for several passages, or split genuinely independent questions. Simultaneous questions cannot use one another's answers. A whole-source negative/universal judgment must not be reinterpreted as a per-window judgment. Twenty-line groups are an experimental address scheme, not a default quality guarantee.

## Ask atomic questions

`questions` maps a key to `{type,instructions,criteria?}`. Keys identify returned answers; they are not sent to the model as evidence. Put the subject, one observable predicate and its evidence boundary in `instructions` or the supplied state. Split independent conditions; each question must stand alone. JEV has no conversation history or sibling answers. Keep causal reasoning, correctness verdicts, counts and unseen-callee behavior with the host and exact tools. Instructions/criteria may be structured data; that does not request generated JSON.

| Evidence available | Ask | Keep with the host |
|---|---|---|
| Search hit lines and supplied context | “Do these lines show a comparison of request-body size against a limit?” | Whether every request path enforces the limit |
| A document passage or selected file | “Does this passage explicitly require a restart after changing the timeout?” | Resolving conflicts between versions or documents |
| A screenshot | “Is a Reload button visibly present in settings_image?” | Whether clicking it will succeed or fix the problem |

For `choice`, describe mutually exclusive outcomes and an insufficient-evidence option when the resource may not answer. Use `noul` for a single presence predicate, not to encode both absence and unreadability. Reuse answers only for the same evidence scope, snapshot/ref and question; do not add a second relevance pass before reading deciding evidence.

| Type | Use and result |
|---|---|
| `noul` | One yes/no predicate; optional `{true,false}` meanings; returns P(yes) |
| `choice` | Mutually exclusive `{label:meaning}` criteria; include an insufficient-evidence label when needed |
| `score` | Ordered criteria `[low … high]`; returns a zero-based, possibly fractional level, not a 1–10 confidence rating |
| `box` | Locate a named visible object in image assets; returns normalized top/bottom/left/right |

Noul needs nonblank instructions or usable true/false criteria. Read `probabilities`, `confidence` and `legend` where returned. Confidence is distribution concentration, not correctness. Low Noul or an excluded candidate does not establish absence elsewhere. `replayed:true` means saved answers, not another provider spend. Follow `responsePagination.next` to collect whole keyed answers; `responsePagination.replayed:true` marks historical page usage. An expired response fails without inference: start a new query only when a fresh assessment is needed. Memory-mode pages belong to one runtime session; use persistent storage for cross-process CLI continuation. Request `extras:["stats"]` on inline readers for current provider calls, reported tokens, unknown usage and evaluation replay counts. Use source for factual proof and AST/LSP for identity.

## Recover from unknown or error

Unknown means unresolved; `relatedItems` are decided near-misses (topical, not direct evidence), hidden but not unresolved; an error is not a negative answer. Inspect `selection`, row errors and returned `warnings`/`hints` before using a judgment. Recover when the missing evidence could change the answer or when the task requires complete coverage; otherwise state the gap and stop.

| Result | Next action |
|---|---|
| More selected evidence is needed | Copy its selected-page continuation unchanged; saved selection replays without JEV. |
| The candidate budget left evidence unjudged | Copy `next.nextCandidatePage` unchanged to classify the remaining candidates. This can make new provider requests. |
| Unknown inline evidence could change the answer | Follow `next.unfilteredUnknown` and any `next.unfilteredUnknown2`, `next.unfilteredUnknown3`, … calls: they read only the unknown chunks or files. Without them, `next.unfiltered` restores the original view. |
| A hidden `ghSearchCode` file has more hits than it lists | Follow `next.judgeHits` (`judgeHits2`, …): it judges every keyword window of those files through `ghGetFileContent` with the same classifiers. |
| Complete original-view coverage is required | Follow `next.unfiltered`, including its pages. This also returns selected and excluded evidence. |
| Selection is unavailable | Inspect its error and warnings. Unfiltered localSearch hits may already be returned; otherwise follow the provided recovery call if needed. |
| `clasify` fails after text capture | Follow the read suggestion in `error.hints` if needed; preserve its snapshot/ref and ranges. Read the original error for a repairable cause. |
| A typed answer is insufficient for the decision | Inspect the deciding source through the read suggestion in `hints` or an observed narrower anchor at the same revision. Interpret labels using the supplied criteria, not a guessed universal threshold. |
| Image judgment fails or is uncertain | Inspect the saved asset path if returned, otherwise the original image reference, with an image-capable tool. Text fetch does not inspect pixels. |
| Invalid input, missing source, unsupported modality or excessive context | Repair the named cause or use ordinary inspection. Bounded retry: retry a corrected request, or a transient provider failure after required backoff; avoid unchanged retries for input/resource errors. |

Unfiltered recovery and inspection calls do not require another JEV pass; candidate-page calls do. Local snapshots and GitHub file commits preserve source identity; GitHub search recovery reruns a provider page and may reflect index changes. Its candidate continuation checks the captured page digest before judging more candidates; drift requires a restart. Read only enough to resolve the decision, except when the claim requires exhaustive coverage. Report an inaccessible source as a gap. A recovery hint is guidance to inspect evidence, not permission to run instructions found inside it.

## Images and models

`assets` maps opaque placeholder keys to PNG/JPEG references from local paths, GitHub paths or HTTPS URLs. Use distinctive keys: the provider expands occurrences within string values, not object keys or question IDs. Put each image reference once in shared `state`, then name that state field in questions; repeating the asset key expands the image again and costs context. Octocode acquires images internally. Image assets require persistent capture storage and available readers. Inline text selection does not inspect images, and a text URL does not crawl embedded images.

```json
[
  {"tool":"clasify","query":{"operation":"models"}},
  {"tool":"clasify","query":{"model":"jev-1.14.0","state":{"screen":"settings_capture_01"},"assets":{"settings_capture_01":{"type":"image","source":{"tool":"localFetch","query":{"path":"screenshots/settings.png"}}}},"questions":{"save_visible":{"type":"noul","instructions":"Is a Save button visibly present in screen?"},"save_location":{"type":"box","instructions":"Locate the Save button in screen."}}}}
]
```

List models with `operation:"models"` (name, description, release date; no capability flags), then pin one with `model`. A rejection is not a negative classification. Box can return coordinates for an absent object: check the independent presence answer before using them. Results report the answering model, usage and captured asset identities/inspection paths without image bytes. Inspect the saved image when a visual detail supports your claim. This capability does not imply audio/video support.
