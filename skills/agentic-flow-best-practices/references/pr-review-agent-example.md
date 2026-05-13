# Worked Example: PR Review Agent

A concocted, build-ready example of a completed flow output. Use as a quality bar when applying the `agentic-flow-best-practices` skill.

---

```markdown
## Flow: PR Review Agent
Goal: Given a GitHub PR, fetch diff and context, classify risk, produce schema-valid
review findings, and gate any comment or memory write.
Framework: LangGraph for checkpoints, human gates, and traceable state transitions.

State Map:
- prNumber        -> input,        one-run,        FlowInput,    request payload
- diff            -> working_state, one-run,        retrieve node, graph state
- riskLevel       -> working_state, one-run,        classify node, graph state
- reviewFindings  -> artifact,      long-lived,     reason node,  file/db reference
- repoFacts       -> memory,        cross-session,  observe node, vector/db store after approval

Cache & Memory:
- Cache: `pr-diff:{owner}/{repo}/{prNumber}@{headSha}`, per-repo, private, invalidate on push
- Memory: never store raw diff; write distilled repo facts only after user approval,
          with retention=90d and delete-on-repo-archive policy

Contracts:
- FlowInput         — validates prNumber, owner, repo, freshness preference
- DiffPayload       — validates raw diff lines, file list, headSha, base
- RiskClassification — discriminated union: low | medium | high | block
- ReviewFindings    — validates findings[], severity[], suggestedFixes[], sources[]
- AgentHandoff      — wraps goal, inputs, knownFacts, constraints, expectedOutput
- CacheEntry        — validates key, inputHash, createdAt, expiresAt, sources[]

Flow:
1. classify: FlowInput -> RiskClassification | fail: stop if invalid PR
2. retrieve: PR + risk -> diff + context chunks (cache: headSha key, TTL 1h) | fail: retry x2
3. reason:   diff + context + riskLevel -> ReviewFindings via LLM | schema fail: retry once, then stop
4. review:   findings preview -> human approval gate | on reject: stop
5. act:      approved findings -> GitHub comment (side effect logged) | fail: retry x2, then stop
6. observe:  approved distilled facts -> memory write + TraceEvent

Context & Tokens:
- Packet to reason: goal + diff (summarized if >500 lines) + top-3 retrieved context chunks
  + riskLevel + constraints (no approve/merge/close) + ReviewFindings schema
- Budget: cache lookup first; retrieve only 3 chunks max; summarize per-file before inline

Prompt Quality (reason node):
- Role: senior engineer with security focus
- Goal: identify correctness bugs, security issues, and style violations in this diff only
- Output schema: ReviewFindings (Zod-validated)
- Failure: if diff is malformed → return parse error, do not guess; if unsure → flag as unknown
- Eval example: diff with hardcoded AWS key → findings must include severity=block + suggestedFix

Gates:
- Before act (post comment): show final review text and require explicit approval
- Before memory write: show distilled facts, retention period, and delete behavior

Verification:
- Schema tests for all 6 contracts
- Cache: hit on same headSha; miss on new push; stale triggers re-fetch
- Memory: "should not remember" test — reject if raw diff stored instead of distilled findings
- Golden trajectory: diff with known SQL injection → severity=high + SQL injection in findings
- Latency budget: classify+retrieve < 3s cached, reason < 8s, full flow < 20s p95
```
