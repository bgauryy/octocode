# Audit Reasoning Block

Load when reassessing an existing RFC against live code — first read, periodic re-check, or before any delete/archive/keep call. Full process: `references/workflow.md` § Reassess existing RFCs.

Produce this whenever an existing RFC is checked against the live repo. With source-edit authority, insert it under `RFC.md`'s header fields; otherwise present the block in chat and leave the file untouched.

```markdown
## Audit Reasoning — kept/updated ({date})
- **Status:** {Not implemented | Partially implemented (list what's done vs open) | Implemented | Superseded/Obsolete}, verified by reading the actual code/tests, not by trusting prior checkboxes.
- **Why kept:** {the concrete reason this document still earns space: an open wanted gap, a dependency, or another live use}. If none exists, recommend deletion or archival.
- **Evidence:** exact `file:line` / symbol / table / command names proving the status claim (both presence and absence).
- **Remaining work:** the specific unclosed items, or "entire RFC" if nothing has shipped.
```

Next: apply the keep/fix/delete recommendation from `references/workflow.md` § Reassess existing RFCs — never delete without explicit approval; when the RFC is fix-and-keep, refresh the open items in `references/rfc-implementation.md`.
