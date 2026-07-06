# RFC.md Template — Decision Body

`RFC.md` is the **decision document**: reviewer-facing, **frozen once accepted**, and the **single source of truth for goals and scope**. `IMPLEMENTATION.md` and `KPI.md` reference its sections — they never restate them.
Keep implementation steps in `rfc-implementation.md` and success metrics in `rfc-kpi.md`.

```markdown
# RFC: {Title}

| Field | Value |
|-------|-------|
| **Status** | Draft / In Review / Accepted / Rejected / Superseded |
| **Decision type** | Reversible (two-way door) / Irreversible (one-way door) |
| **Author(s)** | {names} |
| **Created** | {YYYY-MM-DD} |
| **Last Updated** | {YYYY-MM-DD} |

> Reversible + small ⇒ single-file RFC is fine. Irreversible or wide blast radius ⇒ full folder (RFC + IMPLEMENTATION + KPI).

---

## Summary

One paragraph. A reader understands the core idea after this section alone.

---

## Goals and Non-Goals

- **Goals** — what this RFC commits to achieving (each should be checkable; `KPI.md` will turn these into measurable signals).
- **Non-Goals** — what is explicitly out of scope. Bounding scope here prevents scope creep and anchors the whole document set.

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

Explain the proposal as if already implemented and you were teaching it to another engineer:

- New concepts and terminology, explained by example.
- Sample error messages, deprecation warnings, or migration guidance if applicable.
- How to teach existing vs. new users, and what docs would need reorganizing.

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

Why should we NOT do this? Consider: implementation cost and complexity; whether a simpler approach/config change suffices; maintenance and operational burden; performance impact; learning curve; migration cost (breaking change? codemod?); integration impact; risk of bugs and blast radius. Every proposal has costs — being honest about them builds trust.

### Pre-mortem — how this fails

Assume it's shipped and failed. List the top failure scenarios, each with its trigger and the mitigation now baked into the design or `IMPLEMENTATION.md`. An empty pre-mortem is a red flag, not a strength.

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

---

## Unresolved Questions

Questions this RFC does not yet answer. Each is **closed in `IMPLEMENTATION.md` via octocode research**, or explicitly deferred there with a reason.

**Before acceptance:**
- [ ] {question}

**During implementation:**
- [ ] {question}

**Out of scope:**
- [ ] {question}

> **Tip:** Mark inline open questions anywhere with: `> **Open Question:** {question}`

---

## Future Possibilities

_(Optional)_ Natural extensions out of scope for this RFC. Not a reason to accept the current proposal.
```
