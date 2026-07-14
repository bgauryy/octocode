# Audit Reasoning Block

Load when reassessing an existing RFC against live code — first read, periodic re-check, or before any delete/archive/keep call. Full process: `references/workflow.md` § Reassess existing RFCs.

Required whenever this RFC is (re)read against the live repo instead of drafted fresh. Omit only on a brand-new RFC that has never been checked against code.
Insert directly under `RFC.md`'s header fields (`references/rfc-template.md`) — never a separate file.

```markdown
## Audit Reasoning — kept/updated ({date})
- **Status:** {Not implemented | Partially implemented (list what's done vs open) | Implemented | Superseded/Obsolete}, verified by reading the actual code/tests, not by trusting prior checkboxes.
- **Why kept:** {the concrete reason this document still earns space in `.octocode/rfc/` — an open, wanted gap; still-referenced dependency for another RFC; etc.} If there is no such reason, recommend deletion/archival instead of filling this in.
- **Evidence:** exact `file:line` / symbol / table / command names proving the status claim (both presence and absence).
- **Remaining work:** the specific unclosed items, or "entire RFC" if nothing has shipped.
```
