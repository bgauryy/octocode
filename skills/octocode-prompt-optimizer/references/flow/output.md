# OUTPUT

Load after VALIDATE passes. Why: deliver the requested artifact with a truthful delta. Write only when authorized; otherwise answer in chat.

## Choose A Variant
- Prompt only — no preamble, summary, or rationale — when the request is to write or return a prompt, description, or rule text. Deliver the artifact and nothing else.
- Full optimized document when the user requests a rewrite or leaves the format unspecified. <!-- style-lint: ignore-line the-user -->
- Patch-style delta for minimal edits, review-only work, or unsafe/unavailable writes.
- Contract audit table from `references/tools/contract-audit.md` when the subject is a multi-tool server.

```markdown
# Optimization Complete
## Summary
- Issues: <N>; fixes: <N>; intent preserved: Yes
- Grade: <before> → <after>
- Files changed: <paths or none>

## Changes
| Category | Count | Example / reason |
|---|---:|---|
| <category> | <N> | <bounded description> |

## Optimized Document
<full content; omit for delta mode>

## Patch-Style Delta
| Section | Before | After | Why |
|---|---|---|---|
| <section> | <old> | <new> | <reason> |
```

The variant must match the request, include the deliverable, and report only successful writes. The template above belongs to the document and delta variants; adding it to a prompt-only request contradicts the request. Fix formatting here; if a requested change alters the repair, return to FIX, and revalidate.

## Sources
- Model Context Protocol, [Tools specification](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) — typed result states, structured content, and error signaling support reliable tool use.

Next: the flow ends once you present the artifact and its truthful delta; when the user requests further changes return to `references/flow/fix.md` and revalidate with `references/flow/validate.md`; when a reliability claim still needs proof load `references/flow/evaluation-data.md`. <!-- style-lint: ignore-line the-user -->
