Load when choosing unread evidence, using inline classification or clasify, or recovering from unknown/error results. Live schemas own the exact request types and limits; [shared usage](octocode-usage.md) owns envelopes and channels.

## Choose the route

Use ordinary exact retrieval for a known small target. Narrow noisy text search by path/globs first. Inline classification selects retrieved evidence before host reading; standalone clasify screens unread sources or images with typed answers. Sections assess individual regions across keyed resources. Identity, exhaustive counts and absence claims require ordinary evidence traversal and AST/LSP confirmation.

Discover concrete paths first. A directory is not one source. Before inference, identify the read it could avoid. If classification is unavailable or exact filtering settles the question, continue with ordinary readers. For browser evidence, use the [Chrome workflow](../../octocode-chrome-devtools/SKILL.md).

## Select inside search and fetch

Use `classification:{predicates:["one observable evidence question"],context?:"task context",longContext?:true}` on localSearch, localFetch, ghSearchCode or ghGetFileContent. Questions stand alone; the provider has no conversation history or sibling answers.

```json
{"queries":[{"path":"src/http","matchString":["body"],"contextLines":{"before":3,"after":3},"classification":{"predicates":["Does this reject request bodies above the configured size limit?"]}}]}
```

Search assesses captured matches and supplied context, not hidden whole-file bodies. Fetch assesses selected source windows. `responseChars` controls output size, not provider context. Keep lexical scope broad enough to retrieve deciding evidence. Use enough before/after context, or explicit ranges, when a condition spans several lines.

The selection receipt counts selected, related, excluded and unknown units within its declared scope. Unknown or unavailable is not a negative decision. A selected body can include neighboring context. If it answers the question, stop; otherwise continue the relevant pages or inspect omitted evidence.

## Screen unread sources with clasify

Evaluation uses `operation:"evaluate"`, keyed questions and exactly one of content or source. Content is literal JSON already held; paths inside it are not fetched. Source names an allowed reader/query or supported HTTPS URL. Replace the placeholder below with the upgrade guide's discovered path.

```json
{"queries":[{"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"<discovered-upgrade-guide>"}},"questions":{"mixed_versions":{"type":"noul","instructions":"Does this guide specify when a rolling upgrade can mix server versions?"}}}]}
```

Use returned answers to choose the next read. Read deciding source before making a factual source claim, using the emitted captured read continuation. Typed classification alone may complete a task that asks only for those labels. Save long requests under the skill scratch directory and use CLI --input.

## Assess sections across resources

Use operation sections with keyed resources and keyed questions. Each resource is a local/GitHub reader source or supported URL. Source ranges are typed startLine/endLine objects. Each result resource has status, source identity, section targets, context and coverage. Follow assessment continuations for the scope required by the question; replay must not repeat completed inference.

## Unread images

Assets name source references. Reference their keys in content or questions. The provider receives admitted image bytes; the host receives provenance and an inspection path for saved bytes. Persistent storage, path policy, image limits and HTTPS policy apply. No host base64 payload or private source snapshot field is needed.

## Questions and answers

Question types are noul, choice, score and box. Use live schema for instructions, criteria and answer fields. Ask one observable fact per question. Keep causal conclusions, counts, symbol identity and unseen-callee behavior with the host and exact tools. Provider confidence is not correctness; low relevance is not corpus absence.

## Recover from unknown or error

Inspect row errorCode, selection status and coverage before trusting an answer. Follow saved next calls unchanged when omitted or captured evidence matters. Related calls start optional fresh work. Hints are prose only. Finish responsePagination before assessment or source continuations. An expired or foreign handle fails without inference; start fresh explicitly when needed. Memory handles belong to one runtime.

Transient provider failure may disclose an unfiltered view; refused credentials remain a failure. Provider usage and replay counts distinguish new work from reused judgments. Check source evidence and preserve unresolved scope.
