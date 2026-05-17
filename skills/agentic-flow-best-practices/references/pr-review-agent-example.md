# Worked Example: PR Review Agent

Compact build-ready example.

```markdown
## Flow: PR Review Agent

Goal: Fetch a GitHub PR diff/context, produce schema-valid findings, and post only after approval.
Architecture: Checkpointed workflow/LangGraph. Route is known; durable state and gates matter. Avoid multi-agent until review core works.

State:
- `prRef`: input, owner/repo/number/headSha
- `diff`, `contextChunks`, `risk`: working state
- `findings`: artifact, Zod-valid review output
- `repoFacts`: memory candidates after approval

Protocols (Zod):
- `FlowInput`, `DiffPayload`, `ReviewFinding`, `ReviewOutput`
- `CacheEntry`, `MemoryCandidate`, `HumanGate`, `TraceEvent`, `ErrorEnvelope`

Context packet:
- goal, summarized diff when large, top relevant chunks, risk, repo instructions
- constraints: changed lines plus nearby context; no approve/merge/close/post
- expected output: `ReviewOutput`
- unknowns: missing files/symbols

Cache/memory:
- cache by `pr:{owner}/{repo}/{number}:{headSha}`, scope repo/workspace
- invalidate on new push or changed base
- never store raw private diffs as memory
- write approved conventions/facts only, with source and retention

Flow:
1. `classify`: PR ref -> task/risk; stop if invalid
2. `retrieve`: diff/context; cache by headSha; retry transient fetch
3. `reason`: context packet -> `ReviewOutput`; retry once on schema failure
4. `review`: preview findings; gate post/memory
5. `act`: post approved comment with idempotency key
6. `observe`: trace route, tools, cache, errors, memory candidates

Verify:
- Zod tests for protocols
- cache hit/miss/stale tests
- golden SQL injection PR -> high/block finding
- should-not-remember rejects raw diff/unapproved facts
- tool failure leaves no partial public comment
```
