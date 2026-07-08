# Redemption Flow — Phases 5-6 & Verification

The constructive half of the roast. Reach here after the autopsy in roast-playbook.md.

---

## Phase 5: Redemption Menu

**CRITICAL**: Stop here. Wait for user selection.

```
─────────────────────────────────
      REDEMPTION OPTIONS
─────────────────────────────────

The roast is complete. Choose your penance.

| # | Sin | Fix | Priority |
|---|-----|-----|----------|
| 1 | Hardcoded secrets | Move to env vars + ROTATE KEYS | 🔴 NOW |
| 2 | N+1 queries | Batch query with JOIN | 🔴 NOW |
| 3 | God function | Split into 6 functions | 🟠 HIGH |
| 4 | `any` types | Add proper types | 🟠 HIGH |
| 5 | Callbacks | Convert to async/await | 🟡 MED |

CHOOSE YOUR PATH:

- `1` — Fix single sin
- `1,2,3` — Fix specific sins
- `security` — Fix all security issues (RECOMMENDED FIRST)
- `all` — Full redemption arc
- `shame` — Just roast me more
- `exit` — Stop after the review

What'll it be?
```

---

## Phase 6: Resurrection

Execute chosen fixes with before/after.

```
─────────────────────────────────
      RESURRECTION COMPLETE
─────────────────────────────────

Sins absolved: 4
Files modified: 3
Lines deleted: 412 (good riddance)
Lines added: 187 (quality > quantity)

CHANGES:
✓ Moved credentials to environment variables
  ⚠️ IMPORTANT: Rotate your API keys NOW — they were exposed
✓ Refactored N+1 query to batched JOIN
✓ Split processUserRequest() → 6 focused functions

BEFORE: A cautionary tale
AFTER: Merely concerning

Remaining sins: 6 CRIMES, 11 MISDEMEANORS
(Ask for another pass to continue the redemption arc)
```

---

## Verification Checklist (before delivering)

- [ ] Every roast cites `file:line`
- [ ] No personal attacks, only pattern mockery
- [ ] Security issues (CAPITAL) flagged prominently with action items
- [ ] **Credential values are NEVER output** — report pattern + location, redact the value
- [ ] Fixes are actionable
- [ ] Important findings are separated from redundant or low-value findings
- [ ] Confidence is stated when evidence is partial or inferred
- [ ] User checkpoint before any code modifications
- [ ] Severity matches request and context
- [ ] Humor, if used, stays professionally safe
- [ ] Overflow handled (20+ sins → show top 10)
