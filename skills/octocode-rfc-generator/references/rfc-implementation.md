# RFC Template — Questions, References & Implementation Plan

Continuation of rfc-template.md: the closing sections of the RFC.

```markdown
## Unresolved Questions

**Before acceptance:**
- [ ] {question}

**During implementation:**
- [ ] {question}

**Out of scope:**
- [ ] {question}

**Bikeshedding** _(cosmetic/arbitrary decisions — syntax, naming, formatting — that should not block the proposal)_:
- [ ] {decision}

> **Tip:** Mark inline open questions anywhere in the RFC with: `> **Open Question:** {question}`

---

## Future Possibilities

_(Optional)_ Natural extensions of this proposal. Related ideas that are
out of scope but worth noting. Not a reason to accept the current RFC.

---

## References

Every reference must state **how it supports the RFC thesis**.

### Code References
- [`src/auth/middleware.ts:42`](https://github.com/owner/repo/blob/main/src/auth/middleware.ts#L42) — current token validation; proves §Motivation claim that auth is coupled to HTTP layer
- [`src/cache/store.ts:15-30`](https://github.com/owner/repo/blob/main/src/cache/store.ts#L15-L30) — existing cache pattern; supports §Reference-Level design choice to extend this abstraction

### URLs
- [Express rate-limit benchmarks](https://github.com/express-rate-limit/express-rate-limit/wiki/benchmarks) — proves §Rationale claim that middleware approach scales to 10k req/s
- [Redis vs Memcached comparison (ByteByteGo)](https://blog.bytebytego.com/p/redis-vs-memcached) — supports §Alternatives analysis of cache backends

### Related
- {Links to related RFCs, design docs, or ADRs}

---

## Implementation Plan

### Approach
{Which recommendation is being implemented and why — traces to §Rationale}

### Steps
#### Phase 1: {name}
- [ ] Step — `path/to/file` (ref: §Reference-Level)
- [ ] Step — `path/to/file`

#### Phase 2: {name}
- [ ] Step — `path/to/file`

### Risk Mitigations
{Concrete actions per risk — traces to §Drawbacks}

### Testing Strategy
| Type | Scope | Approach |
|------|-------|----------|
| Unit | {components} | {approach} |
| Integration | {flows} | {approach} |
| Performance | {metrics} | {approach} |

### Rollout Strategy
{Feature flags? Gradual? Big bang? Rollback plan?}
```
