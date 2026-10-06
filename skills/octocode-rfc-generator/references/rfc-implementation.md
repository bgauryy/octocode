# Plan headings

Load when the plan stays inside `RFC.md`, or when it leaves that file as `PLAN.md` or `IMPLEMENTATION.md`. Why: these headings are the plan. Chat and the file use the same names.
Inside `RFC.md`, paste the `##` headings only. A separate file uses the title below.
```markdown
# {Plan | Implementation}: {Title}

## Plan Context
- Primary: {`RFC.md` §Summary / §Rationale | standalone settled direction and evidence}
- Goal (standalone only): {checkable outcome}
- Scope (standalone only): {included and excluded work}
- Constraints: {compatibility, policy, platform, or delivery boundaries}
## Execution Questions
| Non-blocking question (§) | Resolution or explicit deferral | Evidence | Confidence |
|---|---|---|---|
Decision blockers closed before the direction was selected. Each deferral names impact, reason, owner, and revisit trigger.
## Acceptance Contract
Use `KPI.md` anchors when measurement has a separate lifecycle; otherwise complete this table before writing steps.
| Requirement | Pass/fail acceptance | Guardrail or rollback threshold |
|---|---|---|
## Approach
One or two lines linking the settled direction and acceptance contract.
## Steps
Order steps so prerequisites and enabling outputs precede consumers. Every dependency names `none`, a prerequisite anchor, or an earlier step ID.
### Phase 1: {name}
- [ ] S1. {shippable step} — Depends on: {none | prerequisite anchor | earlier step ID} — Produces: {artifact/contract} — Acceptance: {KPI or inline anchor} — Verify: {command/check} — `path:line`
### Phase 2: {name}
- [ ] S2. {next shippable step} — Depends on: {none | prerequisite anchor | earlier step ID} — Produces: {artifact/contract} — Acceptance: {KPI or inline anchor} — Verify: {command/check} — `path:line`
## Files, APIs, and Contracts
| Surface | Change | Blast-radius evidence | Compatibility |
|---|---|---|---|
## Risk Mitigations
| Primary risk/pre-mortem | Preventive action | Detection |
|---|---|---|
## Test and Verification Plan
| Type | Scope | Approach | Command |
|---|---|---|---|
Verification asks whether the build matches design; validation asks whether the acceptance contract moved.
## Rollout, Migration, and Rollback
- Sequence and observable proceed signal
- Trigger: `KPI.md` §Decision Rule when present; otherwise the inline Acceptance Contract
- Flags/canary/percentage gate and rollback procedure
- Owner/approver
```
Gate: reorder or split any step that breaks the lobby step rule. Include estimates only when requested and evidence-based.
Next: cite sources in the sections that use them. Open `references/research-playbook.md` § RESOURCES.md only when the inventory has its own lifecycle, then validate through `references/workflow.md` § Validate and deliver.
