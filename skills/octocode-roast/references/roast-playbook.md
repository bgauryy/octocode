# Roast Playbook — Phases 1-4 (Target → Autopsy)

Execution flow with exact output templates for the destructive half of the roast. After the autopsy, continue with redemption-flow.md for Phases 5-6 (the user checkpoint and fixes) and the pre-delivery verification checklist.

Canonical phases (match SKILL.md): TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM.

```
TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM
         │
         └── If 20+ sins: TRIAGE first (pick top 10)
```

---

## Phase 1: Acquire Target

Auto-detect scope in order:
1. User-specified files, directories, symbols, or line ranges.
2. User-specified PR, branch, or diff scope.
3. Staged files: `git diff --cached --name-only`.
4. Branch diff: `git diff main...HEAD --name-only`.
5. Entire repo only when the user asks for a repo-wide roast.

If the user gave a target, do not silently widen scope. If the target resolves to no files, stop and ask for a corrected target.

**Tactical Scan**:
- Use `octocode-research` for local structure, search, semantic reachability, and blast-radius evidence.
- If `octocode-research` is missing, ask before installing it; otherwise use normal repo tools and mark reduced coverage.
- Treat pattern matches as leads. Upgrade each cited finding with exact `file:line`, impact, confidence, and repair move before writing the roast.

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

`src/api.ts:34` and friends have 12 `any` escapes. At this point
the type system is doing community theater.

There's a try/catch block wrapping 400 lines of code.
The programming equivalent of "thoughts and prayers."

`src/config.ts:47` contains a credential-shaped literal. Rotate if real,
move it to secrets management, and do not claim compromise without evidence.

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
Ask for the full inventory if you want all 27; this pass keeps the signal high.

## 💀 CAPITAL OFFENSES

1. **Hardcoded credentials** — `src/config.ts:47`
   ```ts
   const API_KEY = "sk-live-****" // ⚠️ value redacted — never output secrets
   ```
   Treat as a rotation and removal trigger. Incident claims require evidence.

2. **N+1 Query Bonanza** — `src/api/users.ts:89`
   ```ts
   users.forEach(async user => {
     const orders = await db.query(`SELECT * FROM orders WHERE user_id = ${user.id}`);
   });
   ```
   This is user-visible latency wearing a fake moustache.

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
| Metric | Count | Impact | Confidence |
|--------|-------|--------|------------|
| If statements | 47 | Branching risk around request handling | High |
| Nested depth (max) | 7 | Hard to test failure paths | High |
| WHY comments | 0 | Domain assumptions are hidden | Medium |
| TODO comments | 4 | Deferred behavior needs ownership | Medium |
```

---

Continue with redemption-flow.md for Phase 5 (Redemption Menu — the mandatory user checkpoint), Phase 6 (Resurrection), and the verification checklist.
