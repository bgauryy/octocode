# Roast Playbook — Phases 1-4 (Target → Autopsy)

Execution flow with exact output templates for the destructive half of the roast. After the autopsy, continue with redemption-flow.md for Phases 5-6 (the user checkpoint and fixes) and the pre-delivery verification checklist.

```
TARGET → OBLITERATE → INVENTORY → AUTOPSY → [USER PICKS] → RESURRECT
         │
         └── If 20+ sins: TRIAGE first (pick top 10)
```

---

## Phase 1: Acquire Target

Auto-detect scope in order:
1. Staged files: `git diff --cached --name-only`
2. Branch diff: `git diff main...HEAD --name-only`
3. Specified files/dirs
4. Entire repo (nuclear option)

**Tactical Scan**:
- `localViewStructure` → identify "God Files" (large) and "Dumpster Directories" (too many files).
- `localSearchCode` with `filesOnly=true` → map the blast radius.
- `lspGetSemantics(type=references)` → how far bad patterns spread; `(type=callers/callees)` → trace the infection path.

**Output**:
```
🔥 ROAST INITIATED 🔥

Target acquired: 7 files, 1,247 lines
Threat level: CONCERNING

Scanning for sins...
```

---

## Phase 2: The Opening Salvo

Deliver 3-5 personalized, devastating observations. No generic roasts.

**Template**:
```
─────────────────────────────────
      THE ROAST BEGINS
─────────────────────────────────

*cracks knuckles*

I've reviewed a lot of code. Yours is... certainly some of it.

Your 600-line `handleEverything()` function does exactly what
the name suggests — handles EVERYTHING. Validation, API calls,
state management, probably your taxes. It's not a function,
it's a lifestyle.

You've got 12 `any` types. At this point, just delete your
tsconfig and embrace the chaos you've already chosen.

There's a try/catch block wrapping 400 lines of code.
The programming equivalent of "thoughts and prayers."

Found a hardcoded password on line 47.
Security researchers thank you for your service.

Let's catalog the destruction...
```

---

## Phase 3: Sin Inventory

Categorized, cited, brutal.

**Triage Rule**: If 20+ sins found, present top 10 by severity. Mention overflow count.

**Template**:
```
─────────────────────────────────
      HALL OF SHAME
─────────────────────────────────

Found 27 sins. Showing top 10 (sorted by severity).
Run with `--full` to see all 27 disasters.

## 💀 CAPITAL OFFENSES

1. **Hardcoded credentials** — `src/config.ts:47`
   ```ts
   const API_KEY = "sk-live-****" // ⚠️ value redacted — never output secrets
   ```
   Security incident waiting to happen. Actually, probably already happened.

2. **N+1 Query Bonanza** — `src/api/users.ts:89`
   ```ts
   users.forEach(async user => {
     const orders = await db.query(`SELECT * FROM orders WHERE user_id = ${user.id}`);
   });
   ```
   Your database is filing a restraining order.

## ⚖️ FELONIES

3. **`any` epidemic** — 12 instances (`src/api.ts:34`, `src/utils.ts:89`,
   `src/types.ts:12` — yes, in your TYPES file; the irony is palpable).

─────────────────────────────────
DAMAGE REPORT: 2 CAPITAL | 3 FELONIES | 5 CRIMES | 17 MORE...
─────────────────────────────────
```

---

## Phase 4: Autopsy of Worst Offender

Surgical breakdown of the #1 disaster.

**Template**:
```
─────────────────────────────────
      AUTOPSY REPORT
─────────────────────────────────

🏆 GRAND PRIZE: `processUserRequest()` — 612 lines of ambition

DISSECTION:
Lines 1-80   Input validation → `validateInput()`; 3 try/catch, 2 regex, 1 existential crisis
Lines 81-200 Authentication → `authenticateUser()`; JWT parsing, OAuth, homemade encryption (why?)
Lines 201-400 Business logic → 4-5 domain functions; 47 if statements, 12 else, a switch with 18 cases

METRICS:
| Metric | Count | Verdict |
|--------|-------|---------|
| If statements | 47 | Branching disaster |
| Nested depth (max) | 7 | Pyramid scheme |
| WHY comments | 0 | Mystery meat |
| TODO comments | 4 | Unfulfilled promises |
```

---

Continue with redemption-flow.md for Phase 5 (Redemption Menu — the mandatory user checkpoint), Phase 6 (Resurrection), and the verification checklist.
