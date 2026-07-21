# OUTPUT Gate

Load after VALIDATE passes. Write only when the request authorizes a file change; otherwise present the deliverable in chat.

### Pre-Conditions
- [ ] VALIDATE passed and write authority is known.

## Choose A Variant

- Full optimized document when the user requests a rewrite or leaves format unspecified.
- Patch-style delta for minimal edits, review-only work, or unsafe/unavailable writes.

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

### Gate Check
- [ ] Variant matches the request; summary and required deliverable are present.
- [ ] File-change claims match writes that actually succeeded.

### Forbidden
- Output before validation, omitted deliverable, or false write claims.

### Allowed
- Safe approved write, complete chat rewrite, or concise delta.

### On Failure
- **IF** format alone is wrong → **THEN** regenerate OUTPUT.
- **IF** requested changes alter the fix → **THEN** return to FIX and revalidate.

## Sources
- Model Context Protocol, [Tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) — explicit output structure and error signaling support reliable tool use.
