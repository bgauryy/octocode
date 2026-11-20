# Audit a tool end to end

Run [tool-inventory.mjs](../scripts/tool-inventory.mjs) with the tool name. Follow the [surface map](../references/surface-map.md) from core to each interface, then [contract](../references/contract-audit.md), [implementation](../references/implementation-audit.md) and [output](../references/output-audit.md) in order, covering all three lanes. Fix a confirmed defect at its owning layer per [fix and verify](../references/fix-and-verify.md).

Use the [audit template](../assets/audit-report.md), one report per tool; each 5W2H finding has source location, severity, evidence, fix and verification result, with unverified slots marked unknown. For independent audits give each actor its tool, skill, template and definition of done.
