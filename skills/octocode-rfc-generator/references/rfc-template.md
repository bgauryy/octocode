# RFC Template — Body Sections

Header through Prior Art. For the implementation plan and references sections, see rfc-implementation.md.

```markdown
# RFC: {Title}

| Field | Value |
|-------|-------|
| **Status** | Draft / In Review / Accepted / Rejected / Superseded |
| **Author(s)** | {names} |
| **Created** | {YYYY-MM-DD} |
| **Last Updated** | {YYYY-MM-DD} |

---

## Summary

One paragraph. A reader understands the core idea after this section alone.

---

## Motivation

Why are we doing this? Focus on the problem, not the solution.

- **Problem**: The specific pain or gap
- **Current state**: How the codebase handles this today (include `file:line` refs)
- **Evidence**: Concrete proof the problem exists — logs, metrics, code smells, developer friction
- **Impact**: Who is affected and how
- **Use cases**: Concrete examples where this hurts

This is the most important section. It can be lengthy. If this RFC is not
accepted, the motivation should still be reusable for alternative proposals.

---

## Guide-Level Explanation

Explain the proposal as if it was already implemented and you were teaching
it to another engineer. New concepts, examples, migration guidance.

- What names and terminology work best for these concepts and why?
- How is this idea best presented — as a continuation of existing patterns, or wholly new?
- If applicable, provide sample error messages, deprecation warnings, or migration guidance.
- How should this be taught to existing users vs. new users?
- Would documentation need to be reorganized or altered? Consider API docs, guides, blog posts, tutorials.

---

## Reference-Level Explanation

Technical design detail. Architecture diagrams (Mermaid), API/interface
changes, interaction with existing features, corner cases by example.

For every design choice, include:
- **What** you chose and **why** (link to §Rationale)
- **What you considered and rejected** (link to §Alternatives)
- **What could go wrong** (link to §Drawbacks)
- **Compatibility**: Is this backward-compatible? Breaking changes? Do adopters need a new version? Can it be un-adopted later without breaking existing code?

Enough detail for someone familiar with the codebase to implement it.

---

## Drawbacks

Why should we NOT do this? Consider:

- Implementation cost, both in code size and complexity
- Can this be achieved with existing tools, a simpler approach, or configuration change?
- Maintenance burden and operational overhead
- Performance impact
- Learning curve and impact on teaching
- Migration cost — is this a breaking change? Can we write a codemod?
- Integration impact with other existing and planned features
- Risk of bugs and blast radius

Every proposal has costs. Being honest about them builds trust.

---

## Rationale and Alternatives

### Why This Design?
Why this is the best approach. Every claim must reference research evidence.
Link to specific files, external repos, or benchmarks that support the choice.

### Alternatives Considered (minimum 2)

#### Alternative A: {Name}
- **What**: {description}
- **Evidence**: {GitHub URL or package — where is this used?}
- **Pros / Cons**: {strengths and weaknesses}
- **Why not chosen**: {specific reason}

#### Alternative B: {Name}
- **What / Evidence / Pros / Cons / Why not chosen**

### Comparison Matrix

| Dimension | Proposed | Alt A | Alt B |
|-----------|----------|-------|-------|
| Codebase alignment | | | |
| Implementation complexity | | | |
| Maintenance burden | | | |
| Performance / scalability | | | |
| Community support / maturity | | | |
| Migration effort | | | |

### Trade-off Summary
What are we gaining and what are we giving up with each option? Be explicit.

### What If We Do Nothing?
Impact of not accepting this RFC. This is a valid alternative — give it honest consideration.

---

## Prior Art

What exists already — in the ecosystem, in the codebase, in the literature.
Lessons learned from others. If no prior art, state that explicitly.
```
