Load when designing memory that outlives a task or session.

# Memory

| Type | Content | Persistence |
|---|---|---|
| Procedural | Workflows, policies, skills | Versioned |
| Task | Plan, progress, blockers | Until completion |
| User preference | Explicit durable preferences | Subject to user controls |
| Failure | Failed approaches and their evidence | Task or project scope |

Lifecycle: observe → extract → validate → store → retrieve → use → refresh or forget.

- Store a fact only if a later task would need it and its source was checked; record source, scope and date.
- Before using a stored fact, compare its date and source revision with the current one and re-verify when they differ.
- Provide deletion, expiration, isolation and correction. Validate before writing: unvalidated writes enable memory poisoning.
